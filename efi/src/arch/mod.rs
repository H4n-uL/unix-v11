macro_rules! use_arch {
    ($arch:literal, $modname:ident) => {
        #[cfg(target_arch = $arch)] mod $modname;
        #[cfg(target_arch = $arch)] pub use $modname::*;
    };
}

use_arch!("x86_64", amd64);
use_arch!("aarch64", aarch64);