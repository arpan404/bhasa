//! A single scalar operation exported through the target's C ABI.
//!
//! Build with `-C panic=abort`; see run.sh for the complete command.

/// Doubles `value`, clamping results outside the signed 64-bit range.
///
/// This function accepts every i64 value. It allocates no memory, keeps no
/// state, and passes no references or ownership across the boundary.
#[no_mangle]
pub extern "C" fn bhasa_rust_double(value: i64) -> i64 {
    value.saturating_mul(2)
}
