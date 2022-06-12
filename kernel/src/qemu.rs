use x86_64::instructions::port::Port;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[repr(u32)]
pub enum ExitCode {
    Success = 0x10,
    Failed = 0x11,
}

pub fn exit(exit_code: ExitCode) {
    unsafe {
        // Port defined in Cargo.toml under `package.metadata.bootimage.tast-args`
        let mut port = Port::new(0xf4);
        port.write(exit_code as u32);
    }
}
