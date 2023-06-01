use std::ffi::OsStr;
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::os::raw::c_void;

use std::path::Path;
use std::ptr;
use std::ptr::null_mut;
use std::thread;
use std::time::Duration;
use futures::StreamExt as _;
use napi::{CallContext, JsNull, JsNumber};
use regex::Regex;
use std::sync::mpsc::{Sender, Receiver};
use std::sync::Arc;
use std::time;

#[cfg(target_os = "windows")]
use std::os::windows::io::{AsRawHandle, FromRawHandle};
#[cfg(target_os = "windows")]
use std::os::windows::ffi::OsStrExt;
#[cfg(target_os = "windows")]
use std::os::windows::fs::OpenOptionsExt;
#[cfg(target_os = "windows")]
#[cfg(target_os = "windows")]
use winapi::shared::minwindef::{DWORD, FALSE, TRUE};
#[cfg(target_os = "windows")]
use winapi::shared::minwindef::LPVOID;
#[cfg(target_os = "windows")]
use winapi::shared::winerror::{ERROR_PIPE_CONNECTED, ERROR_PIPE_BUSY, ERROR_SUCCESS};
#[cfg(target_os = "windows")]
use winapi::um::errhandlingapi::GetLastError;
#[cfg(target_os = "windows")]
use winapi::um::fileapi::{ReadFile, WriteFile};
#[cfg(target_os = "windows")]
use winapi::um::fileapi::{CreateFileW, OPEN_EXISTING};
#[cfg(target_os = "windows")]
use winapi::um::handleapi::{CloseHandle, INVALID_HANDLE_VALUE};
#[cfg(target_os = "windows")]
use winapi::um::memoryapi::{
    CreateFileMappingW, FILE_MAP_ALL_ACCESS, MapViewOfFile, OpenFileMappingW, UnmapViewOfFile,
};
#[cfg(target_os = "windows")]
use winapi::um::namedpipeapi::{ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe};
#[cfg(target_os = "windows")]
use winapi::um::synchapi::{
    CreateSemaphoreW, OpenSemaphoreW, ReleaseSemaphore, WaitForSingleObject,
};
#[cfg(target_os = "windows")]
use winapi::um::winbase::{FILE_FLAG_OVERLAPPED, PIPE_ACCESS_DUPLEX, PIPE_TYPE_MESSAGE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT};
#[cfg(target_os = "windows")]
use winapi::um::winbase::{PIPE_READMODE_BYTE, PIPE_TYPE_BYTE};
#[cfg(target_os = "windows")]
use winapi::um::winbase::WAIT_OBJECT_0;
#[cfg(target_os = "windows")]
use winapi::um::winnt::{GENERIC_READ, GENERIC_WRITE};
#[cfg(target_os = "windows")]
use winapi::um::winnt::{FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ};
#[cfg(target_os = "windows")]
use winapi::um::winnt::HANDLE;
#[cfg(target_os = "windows")]
use winapi::um::winnt::PAGE_READWRITE;
#[cfg(target_os = "windows")]
use winapi::um::winnt::SEMAPHORE_ALL_ACCESS;

use futures::StreamExt as _;
use tokio::io::{split, AsyncReadExt, AsyncWriteExt};
use parity_tokio_ipc::{Endpoint, SecurityAttributes};

#[cfg(target_os = "linux")]
extern crate libc;

#[cfg(target_os = "linux")]
use std::ffi::CString;
#[cfg(target_os = "linux")]
use libc::{IPC_CREAT, IPC_EXCL, IPC_RMID, S_IRUSR, S_IWUSR, semctl, semget, semop, SHM_R, SHM_W, c_int, c_long, msgget, msgsnd, msgrcv, shmat, shmctl, shmdt, shmget};

#[cfg(target_os = "linux")]
static SETVAL: i32 = 16;
// cannot found SETVAL definition in libc, define here .
static SEM_UNDO: i16 = 1;

#[cfg(target_os = "linux")]
const KEY: c_int = 1234;
const MSG_SIZE: usize = 1024;

#[repr(C)]
#[cfg(target_os = "linux")]
struct MsgBuf {
    mtype: c_long,
    mtext: [libc::c_char; MSG_SIZE],
}

pub enum Operator {
    CREATE,
    OPEN,
}

///////////////////////////////////////////////////////////////////////////
//  trait define !!!
///////////////////////////////////////////////////////////////////////////
pub trait AbsShm {
    fn create(&mut self, name: &str, size: u32);
    fn open(&mut self, name: &str, size: u32);
    fn close(&self);
    fn read_buf(&self, offset: u32, size: u32) -> Option<&'static [u8]>;
    fn shm_write(&self, offset: u32, buffer: &[u8]);
    fn read_str(&self, offset: u32, size: u32) -> String;
}

pub trait AbsSema {
    fn create(&mut self, name: &str);
    fn open(&mut self, name: &str);
    fn close(&self);
    fn require(&self);
    fn release(&self);
}

pub trait AbsMq {}


///////////////////////////////////////////////////////////////////////////
//  windows share memory implement!!!
///////////////////////////////////////////////////////////////////////////
#[cfg(target_os = "windows")]
pub struct WinShm {
    handler: (LPVOID, HANDLE),
}

#[cfg(target_os = "windows")]
impl AbsShm for WinShm {
    fn create(&mut self, name: &str, size: u32) {
        let handler = shm_create(size);
        self.handler = handler;
    }

    fn open(&mut self, name: &str, size: u32) {
        let handler = shm_open(size);
        self.handler = handler
    }

    fn close(&self) {
        shm_clearup(self.handler);
    }

    fn read_buf(&self, offset: u32, size: u32) -> Option<&'static [u8]> {
        let data = do_shm_read_buf(self.handler.0, offset, size);
        Some(data)
    }

    fn shm_write(&self, offset: u32, buffer: &[u8]) {
        do_shm_write(self.handler.0, offset, buffer);
    }

    fn read_str(&self, offset: u32, size: u32) -> String {
        return do_shm_read_str(self.handler.0, offset, size);
    }
}

#[cfg(target_os = "windows")]
pub fn shm_create(size: u32) -> (LPVOID, HANDLE) {
    let mapping_name = "RustMapping";
    let mapping_size = size.try_into().unwrap();

    let handle = unsafe {
        CreateFileMappingW(
            INVALID_HANDLE_VALUE,
            ptr::null_mut(),
            PAGE_READWRITE,
            0,
            mapping_size,
            mapping_name.encode_utf16().collect::<Vec<_>>().as_ptr(),
        )
    };

    if handle.is_null() {
        log::info!("CreateFileMappingW failed");
    }

    let map = unsafe {
        MapViewOfFile(
            handle,
            FILE_MAP_ALL_ACCESS,
            0,
            0,
            mapping_size as usize,
        )
    };

    if map.is_null() {
        log::info!("MapViewOfFile failed");
    }

    return (map, handle);
}

#[cfg(target_os = "windows")]
pub fn shm_open(size: u32) -> (LPVOID, HANDLE) {
    let mapping_name = "RustMapping";
    let mapping_size: DWORD = size.try_into().unwrap();

    let handle = unsafe {
        OpenFileMappingW(
            FILE_MAP_ALL_ACCESS,
            0,
            mapping_name.encode_utf16().collect::<Vec<_>>().as_ptr(),
        )
    };

    if handle.is_null() {
        log::info!("OpenFileMappingW failed");
    }

    let map = unsafe {
        MapViewOfFile(
            handle,
            FILE_MAP_ALL_ACCESS,
            0,
            0,
            mapping_size as usize,
        )
    };

    if map.is_null() {
        log::info!("MapViewOfFile failed");
    }

    return (map, handle);
}

#[cfg(target_os = "windows")]
pub fn shm_clearup(desc: (LPVOID, HANDLE)) {
    unsafe {
        UnmapViewOfFile(desc.0);
        CloseHandle(desc.1);
    }
}

#[cfg(target_os = "windows")]
fn shm_read_demo(map: LPVOID) {
    let mapping_name = "RustMapping";
    let mapping_size = 1024;

    let handle = unsafe {
        OpenFileMappingW(
            FILE_MAP_ALL_ACCESS,
            false.into(),
            mapping_name.encode_utf16().collect::<Vec<_>>().as_ptr(),
        )
    };

    unsafe {
        log::info!("[0] read last error: {}", GetLastError());
    }

    if handle.is_null() {
        log::info!("OpenFileMappingW failed");
    }

    let map = unsafe {
        MapViewOfFile(
            handle,
            FILE_MAP_ALL_ACCESS,
            0,
            0,
            mapping_size as usize,
        )
    };

    if map.is_null() {
        log::info!("MapViewOfFile failed");
    }

    let buffer = unsafe {
        let slice = std::slice::from_raw_parts(map as *const u8, mapping_size as usize);
        std::str::from_utf8_unchecked(slice)
    };

    unsafe {
        UnmapViewOfFile(map);
        CloseHandle(handle);
    }
}

#[cfg(target_os = "windows")]
pub fn do_shm_write(map: LPVOID, offset: u32, buffer: &[u8]) {
    let data_ptr = buffer.as_ptr() as LPVOID;
    if map.is_null() {
        log::info!("MapViewOfFile failed");
    }

    unsafe {
        let mut dst = map as *mut c_void;
        ptr::copy_nonoverlapping(data_ptr, dst.offset(offset as isize), buffer.len());
    }
}

#[cfg(target_os = "windows")]
pub fn do_shm_read_str(map: LPVOID, offset: u32, size: u32) -> String {
    if map.is_null() {
        log::info!("map is null");
    }

    let buffer = unsafe {
        let src = map as *const u8;
        let slice = std::slice::from_raw_parts(src.offset(offset as isize), size as usize);
        let mut len = slice.len();
        for i in 0..slice.len() {
            if slice[i] == 0 {
                len = i + 1;
                break;
            }
        }
        std::str::from_utf8_unchecked(&slice[..len])
    };

    return String::from(buffer);
}

#[cfg(target_os = "windows")]
pub fn do_shm_read_buf(map: LPVOID, offset: u32, size: u32) -> &'static [u8] {
    if map.is_null() {
        log::info!("map is null");
    }

    unsafe {
        let src = map as *const u8;
        let slice = std::slice::from_raw_parts(src.offset(offset as isize), size as usize);
        slice
    }
}

///////////////////////////////////////////////////////////////////////////
//  windows semaphore implement!!!
///////////////////////////////////////////////////////////////////////////

#[cfg(target_os = "windows")]
pub struct WinSema {
    handler: HANDLE,
}

#[cfg(target_os = "windows")]
impl AbsSema for WinSema {
    fn create(&mut self, name: &str) {
        sema_create(name);
    }

    fn open(&mut self, name: &str) {
        sema_open(name);
    }

    fn close(&self) {
        sema_close(self.handler);
    }

    fn require(&self) {
        sema_require(self.handler);
    }

    fn release(&self) {
        sema_release(self.handler);
    }
}

#[cfg(target_os = "windows")]
pub fn sema_create(name: &str) -> HANDLE {
    let semaphore_name: Vec<u16> = OsStr::new(name)
        .encode_wide()
        .chain(Some(0).into_iter())
        .collect();

    let semaphore_handle: HANDLE =
        unsafe { CreateSemaphoreW(null_mut(), 1, 1, semaphore_name.as_ptr()) };
    if semaphore_handle == null_mut() {
        log::info!("CreateSemaphoreW failed");
    }

    log::info!("Semaphore created");
    return semaphore_handle;
}

#[cfg(target_os = "windows")]
pub fn sema_open(name: &str) -> HANDLE {
    let semaphore_name: Vec<u16> = OsStr::new(name)
        .encode_wide()
        .chain(Some(0).into_iter())
        .collect();

    let semaphore_handle: HANDLE =
        unsafe { OpenSemaphoreW(SEMAPHORE_ALL_ACCESS, false as i32, semaphore_name.as_ptr()) };
    if semaphore_handle == null_mut() {
        log::info!("OpenSemaphoreW failed");
    }
    log::info!("Semaphore opened");
    return semaphore_handle;
}

#[cfg(target_os = "windows")]
pub fn sema_require(semaphore_handle: HANDLE) {
    let wait_result: DWORD = unsafe { WaitForSingleObject(semaphore_handle, 500000000) };
    match wait_result {
        WAIT_OBJECT_0 => {
            log::info!("Semaphore ownership acquired");
        }
        _ => {
            log::info!("WaitForSingleObject failed");
        }
    }
}

#[cfg(target_os = "windows")]
pub fn sema_release(semaphore_handle: HANDLE) {
    let release_result: i32 = unsafe { ReleaseSemaphore(semaphore_handle, 1, null_mut()) };
    if release_result == 0 {
        log::info!("ReleaseSemaphore failed");
    }
}

#[cfg(target_os = "windows")]
pub fn sema_close(semaphore: HANDLE) -> bool {
    let result = unsafe { CloseHandle(semaphore) };

    if result == FALSE {
        return false;
    }

    return true;
}

///////////////////////////////////////////////////////////////////////////
//  windows message queue implement!!!
///////////////////////////////////////////////////////////////////////////

fn str_len(buf: &[u8], max: usize) -> usize {
    for i in 0..max {
        if buf[i] == 0 {
            return i;
        }
    }
    return max;
}

async fn run_server(path: String, sender: Sender<String>, notifier: Sender<bool>) {
    let mut endpoint = Endpoint::new(path);
    endpoint.set_security_attributes(SecurityAttributes::allow_everyone_create().unwrap());
    let incoming = endpoint.incoming().expect("failed to open new socket");
    futures::pin_mut!(incoming);
    notifier.send(true);

    while let Some(result) = incoming.next().await {
        match result {
            Ok(stream) => {
                let (mut reader, mut writer) = split(stream);
                let send_clone = sender.clone();
                tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    loop {
                        for i in 0..1024 {
                            buf[i] = 0;
                        }

                        if let Err(_) = reader.read(&mut buf).await {
                            println!("Closing socket");
                            break;
                        }

                        let size = str_len(&buf, 1024);
                        if size > 0 {
                            let data = std::str::from_utf8(&buf[..size]).unwrap();
                            send_clone.send(String::from(data)).unwrap();
                        }
                    }
                });
            }
            _ => unreachable!("ideally")
        }
    }
}

#[cfg(target_os = "windows")]
#[tokio::main(flavor = "current_thread")]
pub async fn mq_server(topic: String, sender: Sender<String>, notifier: Sender<bool>) {
    let path = format!("\\\\.\\pipe\\ipc_pipe-{}", topic);
    run_server(path, sender, notifier).await
}

#[cfg(target_os = "windows")]
#[tokio::main(flavor = "current_thread")]
pub async fn mq_connect(topic: String, receiver: Receiver<String>, notifier: Sender<bool>) {
    let path = format!("\\\\.\\pipe\\ipc_pipe-{}", topic);

    let mut client = Endpoint::connect(&path).await
        .expect("Failed to connect client.");

    notifier.send(true); // connected ok!

    loop {
        match receiver.recv() {
            Ok(msg) => {
                client.write_all(msg.as_bytes()).await.expect("Unable to write message to client");
            }
            _ => {
                thread::sleep(Duration::from_secs_f32(3.5));
            }
        }
    }
}

#[cfg(target_os = "windows")]
pub fn mq_delete(own_id: u32, target_index: u32) {}

///////////////////////////////////////////////////////////////////////////
//  linux share memory implement!!!
///////////////////////////////////////////////////////////////////////////

#[cfg(target_os = "linux")]
pub struct LinShm {
    handler: *mut c_void,
    shm_id: i32,
}

#[cfg(target_os = "linux")]
impl AbsShm for LinShm {
    fn create(&mut self, name: &str, size: u32) {
        let start = shm_create(name, size);
        self.handler = start.0;
        self.shm_id = start.1
    }

    fn open(&mut self, name: &str, size: u32) {
        let start = shm_open(name, size);
        self.handler = start.0;
        self.shm_id = start.1
    }

    fn close(&self) {
        shm_clearup(self.shm_id);
    }

    fn read_buf(&self, offset: u32, size: u32) -> Option<&'static [u8]> {
        let data = do_shm_read_buf(self.handler, offset, size);
        Some(data)
    }

    fn shm_write(&self, offset: u32, buffer: &[u8]) {
        do_shm_write(self.handler, offset, buffer);
    }

    fn read_str(&self, offset: u32, size: u32) -> String {
        return do_shm_read_str(self.handler, offset, size);
    }
}

///////////////////////////////////////////////////////////////////////////
//  linux semaphore implement!!!
///////////////////////////////////////////////////////////////////////////

#[cfg(target_os = "linux")]
pub struct LinSema {
    sem_id: i32,
}

#[cfg(target_os = "linux")]
impl AbsSema for LinSema {
    fn create(&mut self, name: &str) {
        let sem_id = sema_create(name);
        self.sem_id = sem_id;
    }

    fn open(&mut self, name: &str) {
        let sem_id = sema_open(name);
        self.sem_id = sem_id;
    }

    fn require(&self) {
        sema_require(self.sem_id);
    }

    fn release(&self) {
        sema_release(self.sem_id);
    }

    fn close(&self) {
        sema_close(self.sem_id);
    }
}


#[cfg(target_os = "linux")]
fn name_to_key(name: &str) -> u32 {
    let re = Regex::new(r"-(\d+)-(\d+)").unwrap();
    let caps = re.captures(name).unwrap();
    let num1: u32 = caps.get(1).unwrap().as_str().parse().unwrap();
    let num2: u32 = caps.get(2).unwrap().as_str().parse().unwrap();
    return (num1 + 1) * 10000 + num2;
}

#[cfg(target_os = "linux")]
pub fn sema_create(name: &str) -> i32 {
    let key = name_to_key(name);

    let sem_id = unsafe { semget(key as i32, 1, IPC_CREAT | IPC_EXCL | 0o666) };
    if sem_id == -1 {
        println!("[0] Failed to create semaphore: {}", std::io::Error::last_os_error());
        return -1;
    }
    println!("Semaphore created successfully with id: {}", sem_id);

    let arg = 1;  // semaphore init value!
    let result = unsafe {
        semctl(sem_id, 0, SETVAL, arg)
    };
    if result == -1 {
        println!("Failed to set semaphore value: {}", std::io::Error::last_os_error());
        return -1;
    }
    println!("Semaphore value set successfully");
    return sem_id;
}


#[cfg(target_os = "linux")]
pub fn sema_open(name: &str) -> i32 {
    let key = name_to_key(name);
    let sem_id = unsafe { semget(key as i32, 1, IPC_EXCL | 0o666) };
    if sem_id == -1 {
        println!("[2] Failed to create semaphore: {}", std::io::Error::last_os_error());
        return -1;
    }
    println!("Semaphore created successfully with id: {}", sem_id);

    return sem_id;
}


#[cfg(target_os = "linux")]
#[repr(C)]
pub struct sembuf {
    pub sem_num: libc::c_ushort,
    pub sem_op: libc::c_short,
    pub sem_flg: libc::c_short,
}

#[cfg(target_os = "linux")]
pub fn sema_require(semid: i32) {
    let mut sb = libc::sembuf { sem_num: 0, sem_op: -1, sem_flg: SEM_UNDO };
    let ret = unsafe { semop(semid, &mut sb as *mut libc::sembuf, 1) };
    if ret == -1 {
        panic!("Failed to perform semaphore P operation: {:?}", std::io::Error::last_os_error());
    }
}

#[cfg(target_os = "linux")]
pub fn sema_release(semid: i32) {
    let mut op = libc::sembuf { sem_num: 0, sem_op: 1, sem_flg: SEM_UNDO as i16 };
    let ret = unsafe { semop(semid, &mut op, 1) };
    if ret == -1 {
        println!("semop failed");
        return;
    }
}


#[cfg(target_os = "linux")]
pub fn sema_close(sem_id: i32) {
    let ret = unsafe {
        libc::semctl(sem_id, 0, libc::IPC_RMID, 0)
    };

    if ret == -1 {
        log::info!("Failed to remove semaphore!");
    }
}

#[cfg(target_os = "linux")]
pub fn do_shm_read_buf(map: *mut c_void, offset: u32, size: u32) -> &'static [u8] {
    if map.is_null() {
        log::info!("map is null");
    }

    unsafe {
        let src = map as *const u8;
        let slice = std::slice::from_raw_parts(src.offset(offset as isize), size as usize);
        slice
    }
}

#[cfg(target_os = "linux")]
pub fn do_shm_write(map: *mut c_void, offset: u32, buffer: &[u8]) {
    let data_ptr = buffer.as_ptr();
    if map.is_null() {
        log::info!("MapViewOfFile failed");
    }

    unsafe {
        let dst = map as *mut u8;
        ptr::copy_nonoverlapping(data_ptr, dst.offset(offset as isize), buffer.len());
    }
}

#[cfg(target_os = "linux")]
pub fn do_shm_read_str(map: *mut c_void, offset: u32, size: u32) -> String {
    if map.is_null() {
        log::info!("map is null");
    }

    let buffer = unsafe {
        let src = map as *const u8;
        let slice = std::slice::from_raw_parts(src.offset(offset as isize), size as usize);
        let mut len = slice.len();
        for i in 0..slice.len() {
            if slice[i] == 0 {
                len = i + 1;
                break;
            }
        }
        std::str::from_utf8_unchecked(&slice[..len])
    };

    return String::from(buffer);
}


#[cfg(target_os = "linux")]
pub fn shm_create(name: &str, size: u32) -> (*mut c_void, i32) {
    let shm_id = unsafe { shmget(123456, size as usize, IPC_CREAT | IPC_EXCL | 0o666) };
    if shm_id == -1 {
        eprintln!("[0] Failed to create shared memory: {}", std::io::Error::last_os_error());
        return (null_mut(), 0);
    }

    let shmat_result = unsafe { shmat(shm_id, null_mut(), 0) };

    return (shmat_result, shm_id);
}


#[cfg(target_os = "linux")]
pub fn shm_open(name: &str, size: u32) -> (*mut c_void, i32) {
    let shm_id = unsafe { shmget(123456, 0, IPC_EXCL | 0o666 as i32) };

    if shm_id == -1 {
        eprintln!("[1] Failed to create shared memory: {}", std::io::Error::last_os_error());
        return (null_mut(), 0);
    }

    let shmat_result = unsafe { shmat(shm_id, null_mut(), 0) };
    return (shmat_result, shm_id);
}


#[cfg(target_os = "linux")]
pub fn shm_clearup(shm_id: i32) {
    let ret = unsafe { shmctl(shm_id, IPC_RMID, 0 as *mut libc::shmid_ds) };
    if ret == -1 {
        eprintln!("Failed to delete shared memory: {}", std::io::Error::last_os_error());
    }
}

///////////////////////////////////////////////////////////////////////////
//  linux message queue implement!!!
///////////////////////////////////////////////////////////////////////////
#[cfg(target_os = "linux")]
pub fn mq_server(own_id: u32, peer_id: u32, sender: Sender<String>) {
    // create or get message queue
    let msg_queue_id = unsafe { msgget(KEY + own_id as i32, 0o666 | libc::IPC_CREAT | libc::IPC_EXCL) };
    if msg_queue_id == -1 {
        let err = std::io::Error::last_os_error().raw_os_error().unwrap();
        if err != 17 { // file existed
            println!("Failed to create or get message queue");
            println!("[0] Failed to create msgque: {}", std::io::Error::last_os_error());
        }
        return;
    }

    // receive message
    let mut received_msg_buf = MsgBuf {
        mtype: 0,
        mtext: [0; MSG_SIZE],
    };

    let mut u8_array: [u8; MSG_SIZE] = [0; MSG_SIZE];

    loop {
        let count = unsafe {
            msgrcv(
                msg_queue_id,
                &mut received_msg_buf as *mut MsgBuf as *mut c_void,
                MSG_SIZE,
                0,
                0,
            )
        };

        if count == -1 {
            println!("Failed to receive message");
            return;
        }

        for i in 0..count {
            u8_array[i as usize] = received_msg_buf.mtext[i as usize] as u8;
        }
        u8_array[count as usize] = 0;

        let result = std::str::from_utf8(&u8_array).unwrap();

        sender.send(String::from(result)).unwrap();
    }
}

#[cfg(target_os = "linux")]
pub fn mq_publish(target_index: u32, content: String) {
    // create or get message queue

    let msg_queue_id = unsafe { msgget(KEY + target_index as i32, 0o666) };
    if msg_queue_id == -1 {
        println!("[1] Failed to create or get message queue");
        return;
    }

    // send message
    let mut msg_buf = MsgBuf {
        mtype: 1,
        mtext: [0; MSG_SIZE],
    };

    let msg_text = CString::new(content.as_bytes().to_vec()).unwrap();

    unsafe {
        let len = msg_text.as_bytes().len();
        ptr::copy_nonoverlapping(
            msg_text.as_ptr() as *const libc::c_char,
            msg_buf.mtext.as_mut_ptr(),
            len,
        );

        let result = msgsnd(msg_queue_id, &msg_buf as *const MsgBuf as *const c_void, len, 0);
        if result == -1 {
            println!("Failed to send message");
            return;
        }
        println!("publish to {} {}, ret: {}", target_index, &content, result);
    }
}

#[cfg(target_os = "linux")]
pub fn mq_delete(own_id: u32, target_index: u32) {}

///////////////////////////////////////////////////////////////////////
//  export function !!!
///////////////////////////////////////////////////////////////////////

pub fn abs_shm_create(name: &str, size: u32) -> impl AbsShm {
    #[cfg(target_os = "windows")]
        {
            let mut shm = WinShm {
                handler: (ptr::null_mut(), ptr::null_mut()),
            };
            shm.create(name, size);
            shm
        }

    #[cfg(target_os = "linux")]
        {
            let mut shm = LinShm {
                handler: null_mut(),
                shm_id: 0,
            };
            shm.create(name, size);
            shm
        }
}

pub fn abs_shm_open(name: &str, size: u32) -> impl AbsShm {
    #[cfg(target_os = "windows")]
        {
            let mut shm = WinShm {
                handler: (ptr::null_mut(), ptr::null_mut()),
            };
            shm.open(name, size);
            shm
        }

    #[cfg(target_os = "linux")]
        {
            let mut shm = LinShm {
                handler: null_mut(),
                shm_id: 0,
            };
            shm.open(name, size);
            shm
        }
}

pub fn abs_sema_create(name: &str) -> impl AbsSema {
    #[cfg(target_os = "windows")]
        {
            let mut sema = WinSema {
                handler: ptr::null_mut(),
            };
            sema.create(name);
            sema
        }

    #[cfg(target_os = "linux")]
        {
            let mut sema = LinSema {
                sem_id: 0,
            };
            sema.create(name);
            sema
        }
}

pub fn abs_sema_open(name: &str) -> impl AbsSema {
    #[cfg(target_os = "windows")]
        {
            let mut sema = WinSema {
                handler: ptr::null_mut(),
            };
            sema.open(name);
            sema
        }

    #[cfg(target_os = "linux")]
        {
            let mut sema = LinSema {
                sem_id: 0,
            };
            sema.open(name);
            sema
        }
}