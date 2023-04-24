#[macro_use]
extern crate napi_derive;

use std::collections::HashMap;
use std::io::Error;
use std::path::Path;
use std::process;
use std::ptr;
use std::ptr::null_mut;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::thread;
use std::time;

use futures::prelude::*;
use napi::{bindgen_prelude::*, JsNull, JsString, JsUnknown, threadsafe_function::{ErrorStrategy, ThreadsafeFunction, ThreadsafeFunctionCallMode}};
use napi::{JsFunction, Result};
use napi::bindgen_prelude::*;
use napi::Env;
use napi::threadsafe_function::ThreadSafeCallContext;
use serde::{Deserialize, Serialize};
use serde::de::Unexpected::Option;
use tokio::task;
use winapi::ctypes::c_ulong;
use winapi::shared::minwindef::LPVOID;
use winapi::um::winnt::HANDLE;

use sb::Builder;

mod ipc;
mod sb;

// https://tokio.rs/tokio/tutorial/channels
// https://napi.rs/docs/concepts/async-task
// https://napi.rs/docs/concepts/async-fn
// https://napi.rs/docs/compat-mode/concepts/tokio
// https://docs.rs/interprocess/latest/interprocess/os/windows/named_pipe/enum.PipeDirection.html

#[cfg(target_os = "windows")]
static mut MAP_DESC: (LPVOID, HANDLE) = (ptr::null_mut(), ptr::null_mut());
#[cfg(target_os = "windows")]
//static mut NOTIFY_SEMA: HANDLE = ptr::null_mut();
static SEMA_NAME: &str = "test";
static mut NOTIFY_SEMA_ARR: Vec<HANDLE> = Vec::new();
static mut NOTIFY_SEMA_MAP: Vec<Vec<HANDLE>> = Vec::new();

//  100 message per cell
static MAX_MSG_CNT_PER_CELL: u32 = 100;
//  200 bytes per message!
static MAX_MSG_LEN: u32 = 200;
static mut MAX_WORKER_NUM: u32 = 0;
// sizeof(head_index) + sizeof(tail_index)
static MSG_CELL_META_SIZE: u32 = 8;
static MSG_CELL_SIZE: u32 = MSG_CELL_META_SIZE + MAX_MSG_CNT_PER_CELL * MAX_MSG_LEN;

static mut WORKER_INDEX: u32 = 0;
static mut ready: AtomicBool = AtomicBool::new(false);

#[napi]
pub async fn test_sema_release() {
    task::spawn_blocking(move || {
        unsafe {
            ipc::sema_release(NOTIFY_SEMA_ARR[WORKER_INDEX as usize]);
        }
    }).await.unwrap();
}

#[napi]
pub async fn test_sema_require() {
    task::spawn_blocking(move || {
        unsafe {
            ipc::sema_require(NOTIFY_SEMA_ARR[WORKER_INDEX as usize]);
        }
    }).await.unwrap();
}

fn get_shm_u32(offset: u32) -> u32 {
    unsafe {
        let head_box = ipc::do_shm_read_buf(MAP_DESC.0, offset, 4);
        let mut head_buf: [u8; 4] = [0; 4];
        head_buf.copy_from_slice(&head_box[..4]);
        let head_index = u32::from_be_bytes(head_buf);
        return head_index;
    }
}

fn set_shm_u32(udata: u32, offset: u32) {
    unsafe {
        let buff: [u8; 4] = udata.to_be_bytes();
        ipc::do_shm_write(MAP_DESC.0, offset, &buff);
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
pub async fn test_shm_write(input: String) {
    unsafe {
        let i = WORKER_INDEX; // writer index

        for j in 0..MAX_WORKER_NUM { // reader index
            if i == j {
                continue;
            }

            ipc::sema_require(NOTIFY_SEMA_MAP[i as usize][j as usize]);

            // get the head & tail index
            let meta_offset = meta_offset(i, j);
            let mut tail_index = get_shm_u32(meta_offset + 4); // TODO 修改offset位置
            let buffer = input.as_bytes();
            let offset = msg_offset(i, j, tail_index); // write to tail

            ipc::do_shm_write(MAP_DESC.0, offset, buffer);

            println!("## write: writer pid: {}, tail_index: {}, offset:{}", std::process::id(), tail_index, offset);

            tail_index = tail_index + 1; // tail inc

            // write back the tail index
            set_shm_u32(tail_index, meta_offset + 4);

            ipc::sema_release(NOTIFY_SEMA_MAP[i as usize][j as usize]);
        }
    }
}


#[napi]
pub fn test_shm_read() -> String {
    let out =
        unsafe {
            let mut builder = Builder::default();
            builder.append("[");

            let i = WORKER_INDEX; // reader index

            for j in 0..MAX_WORKER_NUM { // writer index
                if i == j {
                    continue;
                }

                ipc::sema_require(NOTIFY_SEMA_MAP[j as usize][i as usize]);
                // get the head & tail index
                let meta_offset = meta_offset(j, i);

                let mut head_index = get_shm_u32(meta_offset);
                let mut tail_index = get_shm_u32(meta_offset + 4);

                for k in head_index..tail_index {
                    let offset = msg_offset(j, i, k); // read from head
                    let data = ipc::do_shm_read_str(MAP_DESC.0, offset, MAX_MSG_LEN); // read
                    //println!("## read: reader pid: {}, head_index: {}, tail_index: {}, offset: {}", std::process::id(), head_index, tail_index, offset);
                    let empty: [u8; 200] = [0; 200]; // msg length: 200 byts equal to MAX_MSG_LEN
                    ipc::do_shm_write(MAP_DESC.0, offset, &empty); // clear
                    builder.append(data);
                    builder.append(",");
                }

                // write back head & tail index
                head_index = 0;
                tail_index = 0;

                set_shm_u32(head_index, meta_offset);
                set_shm_u32(tail_index, meta_offset + 4);
                ipc::sema_release(NOTIFY_SEMA_MAP[j as usize][i as usize]);
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

#[napi]
pub fn show() -> u32 {
    process::id()
}

enum sema_oper {
    CREATE,
    OPEN,
}

fn init_sema_map(worker_num: u32, oper: sema_oper) {
    unsafe {
        for i in 0..worker_num {
            let mut row: Vec<HANDLE> = Vec::new();
            for j in 0..worker_num {
                if i != j {
                    let sema = match oper {
                        sema_oper::CREATE => {
                            ipc::sema_create(&format!("{}-{}-{}", SEMA_NAME, i, j))
                        }
                        sema_oper::OPEN => {
                            ipc::sema_open(&format!("{}-{}-{}", SEMA_NAME, i, j))
                        }
                    };
                    row.push(sema);
                } else {
                    row.push(ptr::null_mut());
                }
            }
            NOTIFY_SEMA_MAP.push(row);
        }
    }
}

#[napi]
pub async fn master_init(worker_num: u32) {
    unsafe {
        MAX_WORKER_NUM = worker_num;
        let shm_size = MAX_WORKER_NUM * MAX_WORKER_NUM * MSG_CELL_SIZE;
        MAP_DESC = ipc::shm_init(shm_size);
        init_sema_map(worker_num, sema_oper::CREATE);
        ready.store(true, Ordering::SeqCst);
    }
}

fn init_share_memory(worker_num: u32) {}

#[napi]
pub fn worker_init(worker_num: u32, index: u32) {
    thread::spawn(move || {
        unsafe {
            WORKER_INDEX = index;
            MAX_WORKER_NUM = worker_num;
            let shm_size = MAX_WORKER_NUM * MAX_WORKER_NUM * MSG_CELL_SIZE;
            MAP_DESC = ipc::shm_init(shm_size);
            init_sema_map(worker_num, sema_oper::OPEN);
            ready.store(true, Ordering::SeqCst);
        }
    });
}

#[napi]
pub fn process_exit() {
    #[cfg(target_os = "windows")] unsafe {
        ipc::shm_clearup(MAP_DESC);

        for i in 0..MAX_WORKER_NUM {
            for j in 0..MAX_WORKER_NUM {
                if i != j {
                    let sema = NOTIFY_SEMA_MAP[i as usize][j as usize];
                    ipc::sema_close(sema);
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

    println!("data: {}", buffer);
}

#[napi(ts_args_type = "callback: (result: string) => void")]
pub fn call_safe_func(callback: JsFunction) -> Result<()> {
    let tsfn: ThreadsafeFunction<String, ErrorStrategy::Fatal> = callback
        .create_threadsafe_function(0, |ctx: ThreadSafeCallContext<String>| {
            let data = ctx.env.create_string(ctx.value.as_str());
            data.map(|v| vec![v])
        })?;

    let one_millis = time::Duration::from_millis(10);
    thread::spawn(move || {
        loop {
            unsafe {
                let inited = ready.load(Ordering::SeqCst);
                if inited {
                    let data = test_shm_read();
                    tsfn.call(data, ThreadsafeFunctionCallMode::NonBlocking);
                }
                thread::sleep(one_millis);
            }
        }
    });

    Ok(())
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
    println!("#shut down!!")
}

#[napi]
pub fn init(mut env: Env) -> Result<()> {
    env.add_env_cleanup_hook(env, clearup);
    //env.add_async_cleanup_hook(env, clearup);
    Ok(())
}



