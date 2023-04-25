use std::ffi::OsStr;
use std::os::raw::c_void;
use std::os::windows::ffi::OsStrExt;
use std::ptr;
use std::ptr::null_mut;

use futures::StreamExt as _;
use napi::{CallContext, JsNull, JsNumber};

#[cfg(target_os = "windows")]
use winapi::shared::minwindef::{DWORD, FALSE, TRUE};
#[cfg(target_os = "windows")]
use winapi::shared::minwindef::LPVOID;
#[cfg(target_os = "windows")]
use winapi::um::errhandlingapi::GetLastError;
#[cfg(target_os = "windows")]
use winapi::um::handleapi::{CloseHandle, INVALID_HANDLE_VALUE};
#[cfg(target_os = "windows")]
use winapi::um::memoryapi::{CreateFileMappingW, FILE_MAP_ALL_ACCESS, MapViewOfFile, OpenFileMappingW, UnmapViewOfFile};
#[cfg(target_os = "windows")]
use winapi::um::synchapi::{CreateSemaphoreW, OpenSemaphoreW, ReleaseSemaphore, WaitForSingleObject};
#[cfg(target_os = "windows")]
use winapi::um::winbase::WAIT_OBJECT_0;
#[cfg(target_os = "windows")]
use winapi::um::winnt::SEMAPHORE_ALL_ACCESS;
#[cfg(target_os = "windows")]
use winapi::um::winnt::PAGE_READWRITE;
#[cfg(target_os = "windows")]
use winapi::um::winnt::HANDLE;

pub enum Operator {
    CREATE,
    OPEN,
}

pub trait AbsShm {
    fn create(&mut self, name: &str, size: u32);
    fn open(&mut self, name: &str, size: u32);
    fn close(&self);
    fn read_buf(&self, offset: u32, size: u32) -> Option<&'static [u8]>;
    fn shm_write(&self, offset: u32, buffer: &[u8]);
    fn read_str(&self, offset: u32, size: u32) -> String;
    fn clear(&self);
}

pub struct WinShm {
    handler: (LPVOID, HANDLE),
}

pub struct LinShm {

}

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

    fn clear(&self) {
        shm_clearup(self.handler);
    }
}

impl AbsShm for LinShm {
    fn create(&mut self, name: &str, size: u32) {
        todo!()
    }

    fn open(&mut self, name: &str, size: u32) {
        todo!()
    }

    fn close(&self) {
        todo!()
    }

    fn read_buf(&self, offset: u32, size: u32) -> Option<&'static [u8]> {
        None
    }

    fn shm_write(&self, offset: u32, buffer: &[u8]) {

    }

    fn read_str(&self, offset: u32, size: u32) -> String {
        return String::from("");
    }

    fn clear(&self) {

    }
}

pub fn abs_shm_create(name: &str, size: u32) -> impl AbsShm {
    #[cfg(target_os = "windows")] {
        let mut shm = WinShm { handler: (ptr::null_mut(), ptr::null_mut()) };
        shm.create(name, size);
        shm
    }

    #[cfg(target_os = "linux")] {
        None::<LinShm>
    }
}

pub fn abs_shm_open(name: &str, size: u32) -> impl AbsShm {
    #[cfg(target_os = "windows")] {
        let mut shm = WinShm { handler: (ptr::null_mut(), ptr::null_mut()) };
        shm.open(name, size);
        shm
    }

    #[cfg(target_os = "linux")] {
        None::<LinShm>
    }
}

pub trait AbsSema {
    fn create(&mut self, name: &str);
    fn open(&mut self, name: &str);
    fn close(&self);
    fn require(&self);
    fn release(&self);
}

pub struct WinSema {
    handler: HANDLE,
}

pub struct LinSema {

}

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

impl AbsSema for LinSema {
    fn create(&mut self, name: &str) {
        todo!()
    }

    fn open(&mut self, name: &str) {
        todo!()
    }

    fn close(&self) {
        todo!()
    }

    fn require(&self) {

    }

    fn release(&self) {
    }
}

pub fn abs_sema_create(name: &str) -> impl AbsSema {
    #[cfg(target_os = "windows")] {
        let mut sema = WinSema { handler: ptr::null_mut() };
        sema.create(name);
        sema
    }

    #[cfg(target_os = "linux")] {
        None::<WinShm>
    }
}

pub fn abs_sema_open(name: &str) -> impl AbsSema {
    #[cfg(target_os = "windows")] {
        let mut sema = WinSema { handler: ptr::null_mut() };
        sema.open(name);
        sema
    }

    #[cfg(target_os = "linux")] {
        None::<WinShm>
    }
}

pub fn sema_create(name: &str) -> HANDLE {
    // 命名信号量名
    let semaphore_name: Vec<u16> = OsStr::new(name).encode_wide().chain(Some(0).into_iter()).collect();

    // 创建命名信号量，初始计数为0，最大计数为1
    let semaphore_handle: HANDLE = unsafe {
        CreateSemaphoreW(
            null_mut(),
            1,
            1,
            semaphore_name.as_ptr(),
        )
    };
    if semaphore_handle == null_mut() {
        log::info!("CreateSemaphoreW failed");
    }

    log::info!("Semaphore created");
    return semaphore_handle;
}

pub fn sema_open(name: &str) -> HANDLE {
    // 命名信号量名
    let semaphore_name: Vec<u16> = OsStr::new(name).encode_wide().chain(Some(0).into_iter()).collect();

    // 打开命名信号量
    let semaphore_handle: HANDLE = unsafe {
        OpenSemaphoreW(
            SEMAPHORE_ALL_ACCESS,
            false as i32,
            semaphore_name.as_ptr(),
        )
    };
    if semaphore_handle == null_mut() {
        log::info!("OpenSemaphoreW failed");
    }
    log::info!("Semaphore opened");
    return semaphore_handle;
}

pub fn sema_require(semaphore_handle: HANDLE) {
    // 获取信号量所有权
    let wait_result: DWORD = unsafe {
        WaitForSingleObject(semaphore_handle, 500000000)
    };
    match wait_result {
        WAIT_OBJECT_0 => {
            //println!("Semaphore ownership acquired");
            // do something
        }
        _ => {
            log::info!("WaitForSingleObject failed");
        }
    }
}

pub fn sema_release(semaphore_handle: HANDLE) {
    // 释放信号量所有权
    let release_result: i32 = unsafe {
        ReleaseSemaphore(
            semaphore_handle,
            1,
            null_mut(),
        )
    };
    if release_result == 0 {
        log::info!("ReleaseSemaphore failed");
    }
    //println!("Semaphore ownership released");
}

pub fn sema_close(semaphore: HANDLE) -> bool {
    let result = unsafe {
        CloseHandle(semaphore)
    };

    if result == FALSE {
        return false;
    }

    return true;
}


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

    //println!("Read from shared memory: {}", buffer);

    unsafe {
        UnmapViewOfFile(map);
        CloseHandle(handle);
    }
}

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

// data_type: 0 ~ 返回字符串; 1 ~ 返回数组
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

pub fn shm_clearup(desc: (LPVOID, HANDLE)) {
    unsafe {
        UnmapViewOfFile(desc.0);
        CloseHandle(desc.1);
    }
}