use std::collections::HashMap;
use std::sync::Mutex;
use lazy_static::lazy_static;

// Mock storage for our file descriptors and data
lazy_static! {
    static ref MOCK_FILES: Mutex<HashMap<String, Vec<Vec<u8>>>> = Mutex::new(HashMap::new());
    static ref MOCK_FDS: Mutex<HashMap<u32, (String, usize)>> = Mutex::new(HashMap::new());
    static ref NEXT_FD: Mutex<u32> = Mutex::new(1);
}

#[no_mangle]
pub extern "C" fn n_of_streams(name_ptr: *const u8) -> u32 {
    let name = unsafe { std::ffi::CStr::from_ptr(name_ptr as *const i8) }
        .to_string_lossy()
        .to_string();
    
    MOCK_FILES.lock().unwrap()
        .get(&name)
        .map_or(0, |streams| streams.len() as u32)
}

#[no_mangle]
pub extern "C" fn open_read(name_ptr: *const u8, index: u32) -> u32 {
    let name = unsafe { std::ffi::CStr::from_ptr(name_ptr as *const i8) }
        .to_string_lossy()
        .to_string();
    
    let mut fd = NEXT_FD.lock().unwrap();
    let current_fd = *fd;
    *fd += 1;
    
    MOCK_FDS.lock().unwrap().insert(current_fd, (name, index as usize));
    current_fd
}

#[no_mangle]
pub extern "C" fn open_write(name_ptr: *const u8) -> u32 {
    let name = unsafe { std::ffi::CStr::from_ptr(name_ptr as *const i8) }
        .to_string_lossy()
        .to_string();
    
    let mut fd = NEXT_FD.lock().unwrap();
    let current_fd = *fd;
    *fd += 1;
    
    MOCK_FDS.lock().unwrap().insert(current_fd, (name, 0));
    current_fd
}

#[no_mangle]
pub extern "C" fn read(fd: u32, buffer_ptr: *mut u8, count: u32) -> u32 {
    let mock_fds = MOCK_FDS.lock().unwrap();
    let mock_files = MOCK_FILES.lock().unwrap();
    
    if let Some((name, index)) = mock_fds.get(&fd) {
        if let Some(streams) = mock_files.get(name) {
            if let Some(data) = streams.get(*index) {
                let len = std::cmp::min(count as usize, data.len());
                unsafe {
                    std::ptr::copy_nonoverlapping(data.as_ptr(), buffer_ptr, len);
                }
                return len as u32;
            }
        }
    }
    0
}

#[no_mangle]
pub extern "C" fn write(fd: u32, buffer_ptr: *const u8, count: u32) -> u32 {
    let mock_fds = MOCK_FDS.lock().unwrap();
    let mut mock_files = MOCK_FILES.lock().unwrap();
    
    if let Some((name, _)) = mock_fds.get(&fd) {
        let data = unsafe { std::slice::from_raw_parts(buffer_ptr, count as usize) }.to_vec();
        mock_files.entry(name.clone())
            .or_insert_with(Vec::new)
            .push(data);
        return count;
    }
    0
}

#[no_mangle]
pub extern "C" fn close(fd: u32) {
    MOCK_FDS.lock().unwrap().remove(&fd);
}

// Helper functions for tests
#[cfg(test)]
mod test_helpers {
    use super::*;

    pub fn setup_mock_file(name: &str, streams: Vec<Vec<u8>>) {
        MOCK_FILES.lock().unwrap().insert(name.to_string(), streams);
    }

    pub fn clear_mocks() {
        MOCK_FILES.lock().unwrap().clear();
        MOCK_FDS.lock().unwrap().clear();
        *NEXT_FD.lock().unwrap() = 1;
    }
}

#[test]
fn test_file_operations() {
    use test_helpers::*;
    
    clear_mocks();
    
    // Setup test data
    setup_mock_file("test.txt", vec![
        b"stream1".to_vec(),
        b"stream2".to_vec(),
    ]);
    
    // Your test code here
}
