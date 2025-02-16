pub use actor_runtime::*;

const BUFFER_SIZE: usize = 1024;

#[no_mangle]
pub extern "C" fn execute() {
    let input_name = c"";
    let mut buffer = [0u8; BUFFER_SIZE];
    let output_fd = unsafe { open_write(input_name.as_ptr()) };

    // Process each input stream
    let mut i: usize = 0;
    loop {
        let current_n_streams = unsafe { n_of_streams(input_name.as_ptr()) };
        if i >= current_n_streams {
            break;
        }

        let input_fd = unsafe { open_read(input_name.as_ptr(), i) };

        // Copy contents
        loop {
            let bytes_read = unsafe { aread(input_fd, buffer.as_mut_ptr(), BUFFER_SIZE) };
            if bytes_read == 0 {
                break;
            }

            let mut bytes_written = 0;
            while bytes_written < bytes_read {
                let n = unsafe {
                    awrite(
                        output_fd,
                        buffer.as_ptr().add(bytes_written),
                        (bytes_read - bytes_written).try_into().unwrap(),
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
        unsafe { aclose(input_fd) };

        i += 1;
    }

    // Close output stream
    unsafe { aclose(output_fd) };
}
