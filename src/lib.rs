#[macro_use]
extern crate napi_derive;

use std::collections::HashMap;
use std::io::Error;
use std::sync::mpsc::{Sender, Receiver};
use std::sync::Arc;
use std::path::Path;
use std::process;
use std::ptr;
use std::ptr::null_mut;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::thread;
use std::time;
use std::time::Duration;
use std::any::Any;

use futures::prelude::*;
use log::{info, LevelFilter, warn};
use napi::{bindgen_prelude::*, JsNull, JsString, JsUnknown, threadsafe_function::{ErrorStrategy, ThreadsafeFunction, ThreadsafeFunctionCallMode}};
use napi::{JsFunction, Result};
use napi::bindgen_prelude::*;
use napi::Env;
use napi::threadsafe_function::ThreadSafeCallContext;
use simplelog::{CombinedLogger, Config, SimpleLogger, TerminalMode, TermLogger, WriteLogger};
use tokio::task;

use lazy_static::lazy_static;
use std::sync::Mutex;

#[cfg(target_os = "windows")]
use winapi::um::winnt::HANDLE;
#[cfg(target_os = "windows")]
use winapi::shared::minwindef::LPVOID;

use sb::Builder;
use crate::ipc::AbsShm;

mod ipc;
mod sb;


// https://tokio.rs/tokio/tutorial/channels
// https://napi.rs/docs/concepts/async-task
// https://napi.rs/docs/concepts/async-fn
// https://napi.rs/docs/compat-mode/concepts/tokio
// https://docs.rs/interprocess/latest/interprocess/os/windows/named_pipe/enum.PipeDirection.html


static SEMA_NAME: &str = "test";
static mut shm_handler: Option<Box<dyn ipc::AbsShm>> = None;
static mut sema_handler_map: Vec<Vec<Option<Box<dyn ipc::AbsSema>>>> = Vec::new();

//  100 message per cell
static MAX_MSG_CNT_PER_CELL: u32 = 100;
//  200 bytes per message!
static MAX_MSG_LEN: u32 = 200;
static mut MAX_WORKER_NUM: u32 = 0;
static mut IS_MASTER: bool = false;
// sizeof(head_index) + sizeof(tail_index)
static MSG_CELL_META_SIZE: u32 = 8;
static MSG_CELL_SIZE: u32 = MSG_CELL_META_SIZE + MAX_MSG_CNT_PER_CELL * MAX_MSG_LEN;

static mut WORKER_INDEX: u32 = 0;
static mut MSGQUE_INDEX: u32 = 0;

static mut ready: AtomicBool = AtomicBool::new(false);

struct MQSender<T> {
    topic: String,
    sender: Sender<T>,
}

struct MQReceiver<T> {
    topic: String,
    receiver: Receiver<T>,
}

struct SemInfo {
    #[cfg(target_os = "windows")]
    handler: HANDLE,

    #[cfg(target_os = "linux")]
    handler: i32,
}

static mut mq_tx_array: Vec<MQSender<String>> = Vec::new();
static mut mq_rx_array: Vec<MQReceiver<String>> = Vec::new();

static mut sem_map: Option<HashMap<String, SemInfo>> = None;

lazy_static! {
    static ref MAP: Mutex<HashMap<String, String>> = {
        let map:HashMap<String, String> = HashMap::new();
        Mutex::new(map)
    };
}

/*
pub fn put_key(key: String, value: String) {
    MAP.lock().unwrap().insert(key, value);
}

pub fn get_value(key: String) -> Option<String> {
    let guard = MAP.lock().unwrap();
    if let Some(value) = guard.get(&key) {
        return Some(value.to_string());
    }
    return None;
}
*/

fn get_shm_u32(offset: u32) -> u32 {
    unsafe {
        let value = match shm_handler.as_ref().unwrap().read_buf(offset, 4) {
            None => {
                0
            }
            _ => {
                let head_box = shm_handler.as_ref().unwrap().read_buf(offset, 4).unwrap();
                let mut head_buf: [u8; 4] = [0; 4];
                head_buf.copy_from_slice(&head_box[..4]);
                let head_index = u32::from_be_bytes(head_buf);
                head_index
            }
        };
        value
    }
}

fn set_shm_u32(udata: u32, offset: u32) {
    unsafe {
        let buff: [u8; 4] = udata.to_be_bytes();
        shm_handler.as_ref().unwrap().shm_write(offset, &buff)
    }
}

fn msg_offset(writer_index: u32, reader_index: u32, msg_index: u32) -> u32 {
    unsafe {
        let offset = (writer_index * MAX_WORKER_NUM + reader_index) * MSG_CELL_SIZE +
            MSG_CELL_META_SIZE + msg_index * MAX_MSG_LEN; // write to tail
        offset
    }
}

fn meta_offset(writer_index: u32, reader_index: u32) -> u32 {
    unsafe {
        let offset = (writer_index * MAX_WORKER_NUM + reader_index) * MSG_CELL_SIZE;
        offset
    }
}

#[napi]
pub async fn broadcast(input: String) {
    unsafe {
        let i = WORKER_INDEX; // writer index

        for j in 0..MAX_WORKER_NUM { // reader index
            if i == j {
                continue;
            }

            log::info!("## [{}] write sema require [{}][{}]", std::process::id(), i, j);
            sema_handler_map[i as usize][j as usize].as_ref().unwrap().require();

            // get the head & tail index
            let meta_offset = meta_offset(i, j);
            let mut tail_index = get_shm_u32(meta_offset + 4); // TODO 修改offset位置
            let buffer = input.as_bytes();
            let offset = msg_offset(i, j, tail_index); // write to tail

            shm_handler.as_ref().unwrap().shm_write(offset, buffer);

            tail_index = tail_index + 1; // tail inc

            log::info!("## [{}] write tail_index: {}, offset:{}, tail offset:{}", std::process::id(), tail_index, offset, meta_offset + 4);

            // write back the tail index
            set_shm_u32(tail_index, meta_offset + 4);

            log::info!("## [{}] write sema release [{}][{}]", std::process::id(), i, j);
            sema_handler_map[i as usize][j as usize].as_ref().unwrap().release();
        }
    }
}

pub fn proc_shm_read() -> String {
    let out =
        unsafe {
            let mut builder = Builder::default();
            builder.append("[");

            let i = WORKER_INDEX; // reader index

            for j in 0..MAX_WORKER_NUM { // writer index
                if i == j {
                    continue;
                }

                log::info!("## [{}] read sema require [{}][{}]", std::process::id(), j, i);
                sema_handler_map[j as usize][i as usize].as_ref().unwrap().require();

                // get the head & tail index
                let meta_offset = meta_offset(j, i);

                let mut head_index = get_shm_u32(meta_offset);
                let mut tail_index = get_shm_u32(meta_offset + 4);

                log::info!("## [{}] read[0]: reader head_index: {}, tail_index: {}, head offset:{}, tail offset:{}", std::process::id(), head_index, tail_index, meta_offset, meta_offset + 4);
                for k in head_index..tail_index {
                    let offset = msg_offset(j, i, k); // read from head
                    let data = shm_handler.as_ref().unwrap().read_str(offset, MAX_MSG_LEN);

                    log::info!("## [{}] read[1] head_index: {}, tail_index: {}, offset: {}", std::process::id(), head_index, tail_index, offset);
                    let empty: [u8; 200] = [0; 200]; // msg length: 200 byts equal to MAX_MSG_LEN
                    shm_handler.as_ref().unwrap().shm_write(offset, &empty);

                    builder.append(data);
                    builder.append(",");
                }

                // write back head & tail index
                head_index = 0;
                tail_index = 0;

                set_shm_u32(head_index, meta_offset);
                set_shm_u32(tail_index, meta_offset + 4);

                log::info!("## [{}] read sema release [{}][{}]", std::process::id(), j, i);
                sema_handler_map[j as usize][i as usize].as_ref().unwrap().release();
            }


            if builder.len() < 2 {
                builder.append("]");
            } else {
                let ch: u8 = ']' as u32 as u8;
                builder.setByte(builder.len() - 1, ch);
            }
            builder.string().unwrap()
        };
    return out;
}

fn init_sema_map(worker_num: u32, ot: u32) {
    unsafe {
        for i in 0..worker_num {
            let mut row: Vec<Option<Box<dyn ipc::AbsSema>>> = Vec::new();
            for j in 0..worker_num {
                if i != j {
                    if ot == 0 {
                        let sema = ipc::abs_sema_create(&format!("{}-{}-{}", SEMA_NAME, i, j));
                        row.push(Some(Box::new(sema)));
                    }
                    if ot == 1 {
                        let sema = ipc::abs_sema_open(&format!("{}-{}-{}", SEMA_NAME, i, j));
                        row.push(Some(Box::new(sema)));
                    }
                } else {
                    row.push(None);
                }
            }
            sema_handler_map.push(row);
        }
    }
}

#[napi]
pub async fn master_init(worker_num: u32) {
    unsafe {
        sem_map = Some(HashMap::new());
    }

    task::spawn_blocking(move || {
        unsafe {
            MAX_WORKER_NUM = worker_num;
            IS_MASTER = true;
            let shm_size = MAX_WORKER_NUM * MAX_WORKER_NUM * MSG_CELL_SIZE;
            let abs_shm = ipc::abs_shm_create("SHM_NAME", shm_size);
            shm_handler = Some(Box::new(abs_shm));

            init_sema_map(worker_num, 0);

            //mq_data_relay = Some(HashMap::new());

            ready.store(true, Ordering::SeqCst);
        }
    }).await.unwrap();
}

#[napi]
pub async fn worker_init(worker_num: u32, index: u32) {
    let logfile = std::fs::File::create(format!("node_ipc.log")).unwrap();
    let config = Config::default();
    let file_logger = WriteLogger::new(LevelFilter::Info, config, logfile);
    CombinedLogger::init(vec![file_logger]).unwrap();
    unsafe {
        sem_map = Some(HashMap::new());
    }

    task::spawn_blocking(move || {
        unsafe {
            WORKER_INDEX = index;
            MSGQUE_INDEX = index + 1;
            MAX_WORKER_NUM = worker_num;
            let shm_size = MAX_WORKER_NUM * MAX_WORKER_NUM * MSG_CELL_SIZE;

            let abs_shm = ipc::abs_shm_open("SHM_NAME", shm_size);
            shm_handler = Some(Box::new(abs_shm));

            init_sema_map(worker_num, 1);

            ready.store(true, Ordering::SeqCst);
        }
    }).await.unwrap();
}

#[napi]
pub fn process_exit() {
    unsafe {
        shm_handler.as_ref().unwrap().close();
        for i in 0..MAX_WORKER_NUM {
            for j in 0..MAX_WORKER_NUM {
                if i != j {
                    sema_handler_map[i as usize][j as usize].as_ref().unwrap().close();
                }
            }
        }
    }
}

// index: worker process index: from 0
#[napi]
pub fn send_data(index: u32, data: Buffer, n: u32) {
    let buffer = unsafe {
        let slice = std::slice::from_raw_parts(data.as_ptr() as *const u8, n as usize);
        std::str::from_utf8_unchecked(slice)
    };

    log::info!("data: {}", buffer);
}

#[napi(ts_args_type = "callback: (result: string) => void")]
pub fn reg_node_func(callback: JsFunction) -> Result<()> {
    let tsfn: ThreadsafeFunction<String, ErrorStrategy::Fatal> = callback
        .create_threadsafe_function(0, |ctx: ThreadSafeCallContext<String>| {
            let data = ctx.env.create_string(ctx.value.as_str());
            data.map(|v| vec![v])
        })?;

    let delay = time::Duration::from_millis(10);
    thread::spawn(move || {
        loop {
            unsafe {
                let inited = ready.load(Ordering::SeqCst);

                if inited {
                    let data = proc_shm_read();
                    tsfn.call(data, ThreadsafeFunctionCallMode::NonBlocking);
                }
                thread::sleep(delay);
            }
        }
    });

    Ok(())
}

#[napi]
#[cfg(target_os = "windows")]
pub fn sema_create(name: String) {
    unsafe {
        match sem_map.as_ref() {
            Some(map) => {},
            None => {sem_map = Some(HashMap::new())},
        }

        match sem_map.as_ref().unwrap().get(&name) {
            Some(sem_info) => {}
            None => {
                let handler = ipc::sema_create(&name);
                let sem_info = SemInfo { handler };
                sem_map.as_mut().unwrap().insert(String::from(&name), sem_info);
            }
        }
    }
}

#[napi]
#[cfg(target_os = "windows")]
pub fn sema_open(name: String) {
    unsafe {
        match sem_map.as_ref().unwrap().get(&name) {
            Some(sem_info) => {}
            None => {
                let handler = ipc::sema_open(&name);
                let sem_info = SemInfo { handler };
                sem_map.as_mut().unwrap().insert(String::from(&name), sem_info);
            }
        }
    }
}

#[napi]
#[cfg(target_os = "windows")]
pub fn sema_close(name: String) {

    let handler = ipc::sema_create(&name);
    let sem_info = ipc::WinSema { handler };
    let any_val: &dyn std::any::Any = &sem_info;
    let int_val = any_val.downcast_ref::<ipc::WinSema>();

    unsafe {
        match sem_map.as_ref().unwrap().get(&name) {
            Some(sem_info) => {
                ipc::sema_close(sem_info.handler);
            }
            None => {}
        }
    }
}

#[napi]
#[cfg(target_os = "windows")]
pub async fn sema_require(name: String) -> i32 {
    unsafe {
        let ret = match sem_map.as_ref().unwrap().get(&name) {
            Some(sem_info) => {
                ipc::sema_require(sem_info.handler);
                0
            }
            None => {
                1
            }
        };
        return ret;
    }
}

#[napi]
#[cfg(target_os = "windows")]
pub async fn sema_release(name: String) -> i32 {
    unsafe {
        let ret = match sem_map.as_ref().unwrap().get(&name) {
            Some(sem_info) => {
                ipc::sema_release(sem_info.handler);
                0
            }
            None => {
                1
            }
        };
        return ret;
    }
}


// query named pipe in windows
// $ handle -a \Device\NamedPipe | grep -i ipc_pipe
#[napi]
pub async unsafe fn mq_create(topic: String) -> u32 {
    let notifier: (Sender<bool>, Receiver<bool>) = channel();
    let relay: (Sender<String>, Receiver<String>) = channel();
    let relay_tx = relay.0.clone();
    let notifier_tx = notifier.0.clone();

    mq_rx_array.push(MQReceiver { topic: topic.clone(), receiver: relay.1 });

    let t1 = thread::spawn(move || {
        ipc::mq_server(topic, relay_tx, notifier_tx); // 启动mq server
    });

    match notifier.1.recv() {
        Ok(msg) => {}
        _ => {}
    }

    let index = mq_rx_array.len() - 1;
    return index as u32;
}


/**
 * message queue between process map:
 * -------------------------------------
 * master  <- [worker0, worker1, worker2]
 * worker0 <- [master,  worker1, worker2]
 * worker1 <- [worker0, master,  worker2]
 * worker2 <- [worker0, worker1,  master]
 */
#[napi(ts_args_type = "callback: (result: string) => void, index: Number")]
pub unsafe fn listen(callback: JsFunction, index: u32) -> Result<()> {
    let tsfn: ThreadsafeFunction<String, ErrorStrategy::Fatal> = callback
        .create_threadsafe_function(0, |ctx: ThreadSafeCallContext<String>| {
            let data = ctx.env.create_string(ctx.value.as_str());
            data.map(|v| vec![v])
        })?;

    thread::spawn(move || {
        loop {
            unsafe {
                match mq_rx_array.get(index as usize).unwrap().receiver.recv() {
                    Ok(msg) => {
                        tsfn.call(msg, ThreadsafeFunctionCallMode::NonBlocking);
                    }
                    _ => {
                        thread::sleep(Duration::from_secs_f32(3.5));
                    }
                }
            }
        }
    });

    Ok(())
}


#[napi]
#[cfg(target_os = "windows")]
pub async unsafe fn establish(topic: String) -> u32 {
    let notifier: (Sender<bool>, Receiver<bool>) = channel();
    task::spawn_blocking(move || {
        let (tx, rx): (Sender<String>, Receiver<String>) = channel();

        let notifier_tx = notifier.0.clone();

        mq_tx_array.push(MQSender { topic: topic.clone(), sender: tx });

        let t1 = thread::spawn(move || {
            ipc::mq_connect(topic.clone(), rx, notifier_tx); // start mq server
        });
    }).await.unwrap();

    match notifier.1.recv() {
        Ok(msg) => {}
        _ => {}
    }

    let index = mq_tx_array.len() - 1;
    return index as u32;
}

#[napi]
#[cfg(target_os = "linux")]
pub async unsafe fn establish(topic: String) -> u32 {
    return 0;
}

#[napi]
#[cfg(target_os = "windows")]
pub unsafe fn publish(target_index: u32, content: String) {
    mq_tx_array.get(target_index as usize).as_ref().unwrap().sender.send(content);
}

#[napi]
#[cfg(target_os = "linux")]
pub unsafe fn publish(target_index: u32, content: String) {
    ipc::mq_publish(target_index, content);
}

#[napi]
pub async fn call_node_func() -> Result<u32> {
    let one_second = time::Duration::from_secs(1);
    task::spawn_blocking(move || {
        thread::sleep(one_second);
    }).await.unwrap();
    return Ok(100);
}

fn clearup(env: Env) {
    log::info!("#shut down!!")
}

#[napi]
pub fn init(mut env: Env) -> Result<()> {
    env.add_env_cleanup_hook(env, clearup);
    Ok(())
}



