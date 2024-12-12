#[link(wasm_import_module = "")]
extern "C" {
    fn n_of_streams(name_ptr: *const u8) -> u32;
    fn open_read(name_ptr: *const u8, index: u32) -> u32;
    fn open_write(name_ptr: *const u8) -> u32;
    fn read(fd: u32, buffer_ptr: *mut u8, count: u32) -> u32;
    fn write(fd: u32, buffer_ptr: *const u8, count: u32) -> u32;
    fn close(fd: u32);
}

const BUFFER_SIZE: u32 = 1024;

#[no_mangle]
pub extern "C" fn execute() {
    let input_name = b"";
    let mut buffer = [0u8; BUFFER_SIZE as usize];
    let output_fd = unsafe { open_write(input_name.as_ptr()) };

    // Process each input stream
    let mut i = 0;
    loop {
        let current_n_streams = unsafe { n_of_streams(input_name.as_ptr()) };
        if i >= current_n_streams {
            break;
        }

        let input_fd = unsafe { open_read(input_name.as_ptr(), i) };

        // Copy contents
        loop {
            let bytes_read = unsafe { read(input_fd, buffer.as_mut_ptr(), BUFFER_SIZE) };
            if bytes_read == 0 {
                break;
            }

            let mut bytes_written = 0;
            while bytes_written < bytes_read {
                let n = unsafe {
                    write(
                        output_fd,
                        buffer.as_ptr().add(bytes_written as usize),
                        bytes_read - bytes_written,
                    )
                };
                if n == 0 {
                    // Handle write error
                    break;
                }
                bytes_written += n;
            }
        }

        // Close input stream
        unsafe { close(input_fd) };

        i += 1;
    }

    // Close output stream
    unsafe { close(output_fd) };
}
