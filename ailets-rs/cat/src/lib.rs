pub use actor_runtime::*;

const BUFFER_SIZE: usize = 1024;

#[no_mangle]
#[allow(clippy::missing_panics_doc)]
pub extern "C" fn execute() {
    let input_name = c"";
    let mut buffer = [0u8; BUFFER_SIZE];
    let output_fd = unsafe { open_write(input_name.as_ptr()) };

    // Process each input stream
    let mut i: usize = 0;
    loop {
        let current_n_streams = unsafe { n_of_streams(input_name.as_ptr()) };
        if current_n_streams <= i32::try_from(i).unwrap() {
            break;
        }

        let input_fd = unsafe { open_read(input_name.as_ptr(), i) };

        // Copy contents
        loop {
            let bytes_read = unsafe { aread(input_fd, buffer.as_mut_ptr(), BUFFER_SIZE) };
            let bytes_read: usize = match bytes_read {
                -1 => panic!("Failed to read input stream"),
                0 => break,
                n => n.try_into().unwrap(),
            };

            let mut bytes_written: usize = 0;
            while bytes_written < bytes_read {
                let n = unsafe {
                    awrite(
                        output_fd,
                        buffer.as_ptr().add(bytes_written),
                        bytes_read - bytes_written,
                    )
                };
                let n: usize = match n {
                    n if n <= 0 => panic!("Failed to write to output stream"),
                    n => n.try_into().unwrap(),
                };
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
