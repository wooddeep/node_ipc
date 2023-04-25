#[macro_use]
extern crate napi_derive;

use std::collections::HashMap;
use std::io::Error;
use std::path::Path;
use std::process;
use std::ptr;
use std::ptr::null_mut;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::thread;
use std::time;

use futures::prelude::*;
use log::{info, LevelFilter, warn};
use napi::{bindgen_prelude::*, JsNull, JsString, JsUnknown, threadsafe_function::{ErrorStrategy, ThreadsafeFunction, ThreadsafeFunctionCallMode}};
use napi::{JsFunction, Result};
use napi::bindgen_prelude::*;
use napi::Env;
use napi::threadsafe_function::ThreadSafeCallContext;

use simplelog::{CombinedLogger, Config, SimpleLogger, TerminalMode, TermLogger, WriteLogger};
use tokio::task;
#[cfg(target_os = "windows")]
use winapi::ctypes::c_ulong;
#[cfg(target_os = "windows")]
use winapi::shared::minwindef::LPVOID;
#[cfg(target_os = "windows")]
use winapi::um::winnt::HANDLE;

use sb::Builder;
use crate::ipc::AbsShm;

mod ipc;
mod sb;


// https://tokio.rs/tokio/tutorial/channels
// https://napi.rs/docs/concepts/async-task
// https://napi.rs/docs/concepts/async-fn
// https://napi.rs/docs/compat-mode/concepts/tokio
// https://docs.rs/interprocess/latest/interprocess/os/windows/named_pipe/enum.PipeDirection.html

/*
use napi::{CallContext, JsObject, JsString, Result};

// 共享内存的大小
const SHM_SIZE: usize = 1024;

#[cfg(target_os = "linux")]
use libc::{c_void, IPC_CREAT, IPC_EXCL, SHM_R | SHM_W, shmget, shmat, shmdt, shmctl};

#[cfg(target_os = "linux")]
use std::ffi::CString;

// 创建一个共享内存并返回其 id
#[cfg(target_os = "linux")]
fn create_shared_memory(key: &str) -> Result<i32> {
    let key_cstring = CString::new(key).unwrap();

    let shm_id = unsafe {
        shmget(
            key_cstring.as_ptr() as i32,
            SHM_SIZE,
            IPC_CREAT | IPC_EXCL | SHM_R | SHM_W,
        )
    };

    if shm_id == -1 {
        Err(Error::new(
            Status::GenericFailure,
            "Failed to create shared memory",
        ))
    } else {
        Ok(shm_id)
    }
}

// 连接共享内存并返回指向共享内存的指针
#[cfg(target_os = "linux")]
fn attach_shared_memory(shm_id: i32) -> Result<*mut c_void> {
    let shm_ptr = unsafe { shmat(shm_id, std::ptr::null(), 0) };

    if shm_ptr == (0 as *mut c_void) {
        Err(Error::new(
            Status::GenericFailure,
            "Failed to attach shared memory",
        ))
    } else {
        Ok(shm_ptr)
    }
}

// 断开共享内存连接
#[cfg(target_os = "linux")]
fn detach_shared_memory(shm_ptr: *mut c_void) -> Result<()> {
    let status = unsafe { shmdt(shm_ptr) };

    if status == -1 {
        Err(Error::new(
            Status::GenericFailure,
            "Failed to detach shared memory",
        ))
    } else {
        Ok(())
    }
}

// 删除共享内存
#[cfg(target_os = "linux")]
fn delete_shared_memory(shm_id: i32) -> Result<()> {
    let buf = libc::shmid_ds { shm_perm: libc::ipc_perm { __key: 0, mode: 0, uid: 0, gid: 0 } };

    let status = unsafe { shmctl(shm_id, libc::IPC_RMID, &mut buf) };

    if status == -1 {
        Err(Error::new(
            Status::GenericFailure,
            "Failed to delete shared memory",
        ))
    } else {
        Ok(())
    }
}

#[js_function(1)]
pub fn create_shared_memory(ctx: CallContext) -> Result<JsString> {
    let key = ctx.get::<JsString>(0)?.into_utf8()?;

    let shm_id = create_shared_memory(key.as_str()?)?;
    let shm_ptr = attach_shared_memory(shm_id)?;

    // 设置共享内存的内容为 "Hello, world!"
    let hello_world = "Hello, world!";
    unsafe {
        let dest = shm_ptr as *mut u8;
        std::ptr::copy_nonoverlapping(hello_world.as_ptr(), dest, hello_world.len());
    }

    detach_shared_memory(shm_ptr)?;

    Ok(ctx.env.create_string("Shared memory created")?)
}

#[js_function(1)]
pub fn read_shared_memory(ctx: CallContext) -> Result
*/

static SEMA_NAME: &str = "test";
static mut shm_handler: Option<Box<dyn ipc::AbsShm>> = None;
static mut sema_handler_map: Vec<Vec<Option<Box<dyn ipc::AbsSema>>>> = Vec::new();

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
        }
    }).await.unwrap();
}

#[napi]
pub async fn test_sema_require() {
    task::spawn_blocking(move || {
        unsafe {
        }
    }).await.unwrap();
}

fn get_shm_u32(offset: u32) -> u32 {
    unsafe {
        let head_box = shm_handler.as_ref().unwrap().read_buf(offset, 4).unwrap();
        let mut head_buf: [u8; 4] = [0; 4];
        head_buf.copy_from_slice(&head_box[..4]);
        let head_index = u32::from_be_bytes(head_buf);
        return head_index;
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
pub async fn test_shm_write(input: String) {
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
                    if ot ==1  {
                        let sema = ipc::abs_sema_open(&format!("{}-{}-{}", SEMA_NAME, i, j));
                        row.push(Some( Box::new(sema)));
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
        MAX_WORKER_NUM = worker_num;
        let shm_size = MAX_WORKER_NUM * MAX_WORKER_NUM * MSG_CELL_SIZE;
        let abs_shm = ipc::abs_shm_create("SHM_NAME", shm_size);
        shm_handler = Some(Box::new(abs_shm));

        init_sema_map(worker_num, 0);
        ready.store(true, Ordering::SeqCst);
    }
}

#[napi]
pub fn worker_init(worker_num: u32, index: u32) {
    let logfile = std::fs::File::create(format!("node_ipc.log")).unwrap();
    let config = Config::default();
    let file_logger = WriteLogger::new(LevelFilter::Info, config, logfile);
    CombinedLogger::init(vec![file_logger]).unwrap();

    thread::spawn(move || {
        unsafe {
            WORKER_INDEX = index;
            MAX_WORKER_NUM = worker_num;
            let shm_size = MAX_WORKER_NUM * MAX_WORKER_NUM * MSG_CELL_SIZE;

            let abs_shm = ipc::abs_shm_open("SHM_NAME", shm_size);
            shm_handler = Some(Box::new(abs_shm));

            init_sema_map(worker_num, 1);
            ready.store(true, Ordering::SeqCst);
        }
    });
}



#[napi]
pub fn process_exit() {
    #[cfg(target_os = "windows")] unsafe {
        shm_handler.as_ref().unwrap().clear();
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
pub fn call_safe_func(callback: JsFunction) -> Result<()> {
    let tsfn: ThreadsafeFunction<String, ErrorStrategy::Fatal> = callback
        .create_threadsafe_function(0, |ctx: ThreadSafeCallContext<String>| {
            let data = ctx.env.create_string(ctx.value.as_str());
            data.map(|v| vec![v])
        })?;

    let delay = time::Duration::from_millis(2000);
    thread::spawn(move || {
        loop {
            unsafe {
                let inited = ready.load(Ordering::SeqCst);
                if inited {
                    let data = test_shm_read();
                    tsfn.call(data, ThreadsafeFunctionCallMode::NonBlocking);
                }
                thread::sleep(delay);
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
    log::info!("#shut down!!")
}

#[napi]
pub fn init(mut env: Env) -> Result<()> {
    env.add_env_cleanup_hook(env, clearup);
    //env.add_async_cleanup_hook(env, clearup);
    Ok(())
}



