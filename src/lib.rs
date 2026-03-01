use std::ptr::null_mut;

mod bindings {
    #![allow(nonstandard_style)]
    #![allow(unused)]
    #![allow(unnecessary_transmutes)]
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

    #[cfg(target_arch = "wasm32")]
    unsafe extern "C" {
        pub fn mrc_ccontext_new(mrb: *mut ::std::os::raw::c_void) -> *mut mrc_ccontext;
        pub fn mrc_ccontext_free(c: *mut mrc_ccontext);
        pub fn mrc_load_string_cxt(
            c: *mut mrc_ccontext,
            source: *mut *const u8,
            length: usize,
        ) -> *mut mrc_irep;
        pub fn mrc_dump_irep(
            c: *mut mrc_ccontext,
            irep: *const mrc_irep,
            flags: u8,
            bin: *mut *mut u8,
            bin_size: *mut usize,
        ) -> ::std::os::raw::c_int;
        pub fn mrc_irep_free(c: *mut mrc_ccontext, irep: *mut mrc_irep);
    }

    #[cfg(all(target_arch = "wasm32", feature = "std"))]
    unsafe extern "C" {
        pub fn fdopen(
            arg1: ::std::os::raw::c_int,
            arg2: *const ::std::os::raw::c_char,
        ) -> *mut FILE;
        pub fn mrc_codedump_all(c: *mut mrc_ccontext, irep: *mut mrc_irep);
        pub fn mrc_dump_irep_cfunc(
            c: *mut mrc_ccontext,
            irep: *const mrc_irep,
            flags: u8,
            fp: *mut FILE,
            initname: *const ::std::os::raw::c_char,
        ) -> ::std::os::raw::c_int;
    }
}
use bindings::{
    MRC_DUMP_OK, mrc_ccontext, mrc_ccontext_free, mrc_ccontext_new, mrc_dump_irep, mrc_irep,
    mrc_irep_free, mrc_load_string_cxt,
};

#[cfg(feature = "std")]
use std::os::unix::io::AsRawFd;

#[cfg(feature = "std")]
use bindings::{FILE, fdopen, mrc_codedump_all, mrc_dump_irep_cfunc};

#[derive(Debug)]
pub struct MRubyCompiler2Error {
    details: String,
}

impl MRubyCompiler2Error {
    fn new(msg: &str) -> MRubyCompiler2Error {
        MRubyCompiler2Error {
            details: msg.to_string(),
        }
    }

    #[allow(unused)]
    fn from_error<E: std::error::Error>(msg: &str, err: E) -> MRubyCompiler2Error {
        MRubyCompiler2Error {
            details: format!("{}: {}", msg, err.to_string()),
        }
    }
}

impl std::fmt::Display for MRubyCompiler2Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.details)
    }
}

impl std::error::Error for MRubyCompiler2Error {}

pub struct MRubyCompiler2Context {
    c: *mut mrc_ccontext,
}

impl MRubyCompiler2Context {
    /// Creates a new MRubyCompiler2Context
    pub unsafe fn new() -> Self {
        unsafe {
            let ccontext = mrc_ccontext_new(null_mut());
            MRubyCompiler2Context { c: ccontext }
        }
    }

    /// Compiles the given mruby code into mruby bytecode binary
    /// Returns the bytecode as a `Vec<u8>`
    pub unsafe fn compile(&mut self, code: &str) -> Result<Vec<u8>, MRubyCompiler2Error> {
        unsafe {
            let c_code = std::ffi::CString::new(code)
                .map_err(|_| MRubyCompiler2Error::new("Code includes null bytes"))?;
            let mut ptr = c_code.as_ptr() as *const u8;
            let irep =
                mrc_load_string_cxt(self.c, &mut ptr as *mut *const u8, c_code.as_bytes().len());

            if irep.is_null() {
                return Err(MRubyCompiler2Error::new("Failed to compile code"));
            }

            // Set dummy capacity, deduced from code length
            // And leak for safety rather than memory efficiency
            let bin: &'static mut [u8] = Vec::with_capacity(code.len() * 2).leak();
            let bin_ptr = bin.as_mut_ptr();
            let mut bin_size: usize = 0;

            let result = mrc_dump_irep(
                self.c,
                irep as *mut mrc_irep,
                0,
                &bin_ptr as *const *mut u8 as *mut *mut u8,
                &mut bin_size as *mut usize,
            );
            mrc_irep_free(self.c, irep as *mut mrc_irep);
            if result as u32 != MRC_DUMP_OK {
                return Err(MRubyCompiler2Error::new("Failed to dump irep binary"));
            }

            let newvec = Vec::from_raw_parts(bin_ptr, bin_size, bin_size);
            Ok(newvec)
        }
    }

    /// Dumps the compiled bytecode of the given mruby code to stdout
    #[cfg(feature = "std")]
    pub unsafe fn dump_bytecode(&mut self, code: &str) -> Result<(), MRubyCompiler2Error> {
        unsafe {
            let c_code = std::ffi::CString::new(code)
                .map_err(|_| MRubyCompiler2Error::new("Code includes null bytes"))?;
            let mut ptr = c_code.as_ptr() as *const u8;
            let irep =
                mrc_load_string_cxt(self.c, &mut ptr as *mut *const u8, c_code.as_bytes().len());

            if irep.is_null() {
                return Err(MRubyCompiler2Error::new("Failed to compile code"));
            }

            mrc_codedump_all(self.c, irep as *mut mrc_irep);
            mrc_irep_free(self.c, irep as *mut mrc_irep);
            Ok(())
        }
    }

    /// Compiles the given mruby code and writes the bytecode to the specified file path
    #[cfg(feature = "std")]
    pub unsafe fn compile_to_file(
        &mut self,
        code: &str,
        path: &std::path::Path,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let bin = unsafe { self.compile(code) }?;
        let mut out = std::fs::File::create(path)?;
        std::io::Write::write_all(&mut out, &bin)?;
        Ok(())
    }

    /// Compiles the given mruby code and writes the bytecode as a C function to the specified file path
    #[cfg(feature = "std")]
    pub unsafe fn compile_to_c_function(
        &mut self,
        code: &str,
        initname: &str,
        path: &std::path::Path,
    ) -> Result<(), MRubyCompiler2Error> {
        let out = std::fs::File::create(path)
            .map_err(|e| MRubyCompiler2Error::from_error("Failed to create file", e))?;

        unsafe {
            let c_code = std::ffi::CString::new(code)
                .map_err(|e| MRubyCompiler2Error::from_error("Code includes null bytes", e))?;
            let mut ptr = c_code.as_ptr() as *const u8;
            let irep =
                mrc_load_string_cxt(self.c, &mut ptr as *mut *const u8, c_code.as_bytes().len());

            if irep.is_null() {
                return Err(MRubyCompiler2Error::new("Failed to compile code"));
            }
            let fd = out.as_raw_fd();
            let mode_str = std::ffi::CString::new("w").unwrap();
            let fp = fdopen(fd, mode_str.as_ptr());
            std::mem::forget(out);

            let initname = std::ffi::CString::new(initname)
                .map_err(|e| MRubyCompiler2Error::from_error("Initname includes null bytes", e))?;

            let result = mrc_dump_irep_cfunc(self.c, irep, 0, fp as *mut FILE, initname.as_ptr());
            mrc_irep_free(self.c, irep as *mut mrc_irep);
            if result as u32 != MRC_DUMP_OK {
                return Err(MRubyCompiler2Error::new("Failed to dump irep binary"));
            }
            Ok(())
        }
    }
}

impl Drop for MRubyCompiler2Context {
    fn drop(&mut self) {
        unsafe {
            mrc_ccontext_free(self.c);
        }
    }
}
