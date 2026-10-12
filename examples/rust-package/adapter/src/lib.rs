/// A scalar C ABI adapter; no references or ownership cross this boundary.
#[no_mangle]
pub extern "C" fn adapter_double(value: i64) -> i64 {
    value.saturating_mul(2)
}
