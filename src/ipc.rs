use std::ffi::OsStr;
use std::os::raw::c_void;
use std::os::windows::ffi::OsStrExt;
use std::ptr;
use std::ptr::null_mut;

use futures::StreamExt as _;
use napi::{CallContext, JsNull, JsNumber};
use winapi::shared::minwindef::{DWORD, FALSE, TRUE};
use winapi::shared::minwindef::LPVOID;
use winapi::um::errhandlingapi::GetLastError;
use winapi::um::handleapi::{CloseHandle, INVALID_HANDLE_VALUE};
use winapi::um::memoryapi::{CreateFileMappingW, FILE_MAP_ALL_ACCESS, MapViewOfFile, OpenFileMappingW, UnmapViewOfFile};
use winapi::um::synchapi::{CreateSemaphoreW, OpenSemaphoreW, ReleaseSemaphore, WaitForSingleObject};
use winapi::um::winbase::WAIT_OBJECT_0;
use winapi::um::winnt::SEMAPHORE_ALL_ACCESS;
use winapi::um::winnt::PAGE_READWRITE;
use winapi::um::winnt::HANDLE;

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
        println!("CreateSemaphoreW failed");
    }

    println!("Semaphore created");
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
        println!("OpenSemaphoreW failed");
    }
    println!("Semaphore opened");
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
            println!("WaitForSingleObject failed");
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
        println!("ReleaseSemaphore failed");
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
        println!("[0] read last error: {}", GetLastError());
    }

    if handle.is_null() {
        panic!("OpenFileMappingW failed");
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
        panic!("MapViewOfFile failed");
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
        panic!("MapViewOfFile failed");
    }

    unsafe {
        let mut dst = map as *mut c_void;
        ptr::copy_nonoverlapping(data_ptr, dst.offset(offset as isize), buffer.len());
    }
}

// data_type: 0 ~ 返回字符串; 1 ~ 返回数组
pub fn do_shm_read_str(map: LPVOID, offset: u32, size: u32) -> String {
    if map.is_null() {
        panic!("map is null");
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
        panic!("map is null");
    }

    unsafe {
        let src = map as *const u8;
        let slice = std::slice::from_raw_parts(src.offset(offset as isize), size as usize);
        slice
    }
}

pub fn shm_init(size: u32) -> (LPVOID, HANDLE) {
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
        panic!("CreateFileMappingW failed");
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
        panic!("MapViewOfFile failed");
    }

    return (map, handle);
}

pub fn shm_clearup(desc: (LPVOID, HANDLE)) {
    unsafe {
        UnmapViewOfFile(desc.0);
        CloseHandle(desc.1);
    }
}