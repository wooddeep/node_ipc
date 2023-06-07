#[macro_use]
extern crate napi_derive;

use std::any::Any;
use std::collections::HashMap;
use std::io::Error;
use std::os::raw::c_void;
use std::path::Path;
use std::process;
use std::ptr;
use std::ptr::null_mut;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::mpsc::channel;
use std::sync::Mutex;
use std::thread;
use std::time;
use std::time::Duration;

use futures::prelude::*;
use lazy_static::lazy_static;
use log::{info, LevelFilter, warn};
use napi::{bindgen_prelude::*, JsNull, JsString, JsUnknown, threadsafe_function::{ErrorStrategy, ThreadsafeFunction, ThreadsafeFunctionCallMode}};
use napi::{JsFunction, Result};
use napi::bindgen_prelude::*;
use napi::Env;
use napi::threadsafe_function::ThreadSafeCallContext;
use simplelog::{CombinedLogger, Config, SimpleLogger, TerminalMode, TermLogger, WriteLogger};
use tokio::task;
#[cfg(target_os = "windows")]
use winapi::shared::minwindef::LPVOID;
#[cfg(target_os = "windows")]
use winapi::um::winnt::HANDLE;

use sb::Builder;

use crate::ipc::AbsShm;

mod ipc;
mod sb;
mod test;

// https://docs.rs/interprocess/latest/interprocess/os/windows/named_pipe/enum.PipeDirection.html
// https://tokio.rs/tokio/tutorial/channels
// https://napi.rs/docs/concepts/async-task
// https://napi.rs/docs/concepts/async-fn
// https://napi.rs/docs/compat-mode/concepts/tokio

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

struct ShmInfo {
    #[cfg(target_os = "windows")]
    handler: HANDLE,
    #[cfg(target_os = "windows")]
    address: LPVOID,

    #[cfg(target_os = "linux")]
    handler: i32,
    #[cfg(target_os = "linux")]
    address: *mut c_void,
}

static mut mq_tx_array: Vec<MQSender<String>> = Vec::new();
static mut mq_rx_array: Vec<MQReceiver<String>> = Vec::new();
static mut sem_map: Option<HashMap<String, SemInfo>> = None;
static mut shm_map: Option<HashMap<String, ShmInfo>> = None;

#[napi]
pub fn process_exit() {
    unsafe {

    }
}

#[napi]
pub fn sem_create(name: String) {
    unsafe {
        match sem_map.as_ref() {
            Some(map) => {}
            None => { sem_map = Some(HashMap::new()) }
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
pub fn sem_open(name: String) {
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
pub fn sem_close(name: String) {
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
pub async fn sem_require(name: String) -> i32 {
    unsafe {

        let ret = match sem_map.as_ref() {
            Some(map) => {}
            None => { sem_map = Some(HashMap::new()) }
        };

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
pub async fn sem_release(name: String) -> i32 {
    unsafe {
        let ret = match sem_map.as_ref() {
            Some(map) => {}
            None => { sem_map = Some(HashMap::new()) }
        };

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

// share memorey
#[napi]
pub fn shm_create(name: String, size: u32) {
    unsafe {
        match shm_map.as_ref() {
            Some(map) => {}
            None => { shm_map = Some(HashMap::new()) }
        }

        match shm_map.as_ref().unwrap().get(&name) {
            Some(sem_info) => {}
            None => {
                let handler = ipc::shm_create(size, name.clone());
                let shm_info = ShmInfo { handler: handler.1, address: handler.0 };
                shm_map.as_mut().unwrap().insert(name, shm_info);
            }
        }
    }
}

#[napi]
pub fn shm_open(name: String, size: u32) {
    unsafe {
        match shm_map.as_ref() {
            Some(map) => {}
            None => {
                shm_map = Some(HashMap::new())
            }
        }

        match shm_map.as_ref().unwrap().get(&name) {
            Some(sem_info) => {}
            None => {
                let handler = ipc::shm_open(size, name.clone());
                let shm_info = ShmInfo { handler: handler.1, address: handler.0 };
                shm_map.as_mut().unwrap().insert(name, shm_info);
            }
        }
    }
}

#[napi]
pub fn shm_close(name: String) {
    unsafe {
        match shm_map.as_ref().unwrap().get(&name) {
            Some(shm_info) => {
                ipc::shm_clearup((shm_info.address, shm_info.handler));
            }
            None => {}
        }
    }
}

#[napi]
pub fn shm_read_buf(name: String, offset: u32, size: u32) -> Option<&'static [u8]> {
    let ret = unsafe {
        match shm_map.as_ref().unwrap().get(&name) {
            Some(shm_info) => {
                let out = ipc::do_shm_read_buf(shm_info.address, offset, size);
                Some(out)
            }
            None => { None }
        }
    };
    return ret;
}

#[napi]
pub fn shm_read_str(name: String, offset: u32, size: u32) -> Option<String> {
    let ret = unsafe {
        match shm_map.as_ref().unwrap().get(&name) {
            Some(shm_info) => {
                let out = ipc::do_shm_read_str(shm_info.address, offset, size);
                Some(out)
            }
            None => { None }
        }
    };
    return ret;
}

#[napi]
pub fn shm_write_str(name: String, offset: u32, data: String) {
    let ret = unsafe {
        match shm_map.as_ref().unwrap().get(&name) {
            Some(shm_info) => {
                ipc::do_shm_write(shm_info.address, offset, data.as_bytes());
            }
            None => {}
        }
    };
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

#[napi(ts_args_type = "callback: (result: string) => void, index: Number")]
pub unsafe fn mq_listen(callback: JsFunction, index: u32) -> Result<()> {
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
pub async unsafe fn mq_establish(topic: String) -> u32 {
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
pub async unsafe fn mq_establish(topic: String) -> u32 {
    return ipc::str_to_key(&topic) as u32;
}

#[napi]
#[cfg(target_os = "windows")]
pub unsafe fn mq_publish(target_index: u32, content: String) {
    mq_tx_array.get(target_index as usize).as_ref().unwrap().sender.send(content);
}

#[napi]
#[cfg(target_os = "linux")]
pub unsafe fn mq_publish(key: u32, content: String) {
    ipc::mq_publish(key, content);
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


