//! Flowmark in the browser: plain C-ABI exports, no wasm-bindgen.
//!
//! Protocol (see `web/flowmark-wasm.js`):
//! 1. JS asks for input buffers with `fm_alloc(len)`, writes UTF-8 into them.
//! 2. JS calls a function with `(ptr, len)` pairs. The function takes ownership of the inputs and frees them.
//! 3. The function returns the byte length of its result; `fm_result_ptr()` points at it until the next call.
//!
//! The parser never frees its syntax trees (tsrs's arena), so the JS side creates a new instance when the
//! memory has grown too much.

use std::cell::RefCell;

use flowmark_parser::{parse, view, viewmodel};

thread_local! {
    static RESULT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

fn set_result(s: String) -> u32 {
    RESULT.with(|r| {
        *r.borrow_mut() = s.into_bytes();
        r.borrow().len() as u32
    })
}

/// Takes ownership of a buffer from `fm_alloc`.
///
/// # Safety
/// `ptr`/`len` must come from `fm_alloc(len)` and must not be used again.
unsafe fn take(ptr: *mut u8, len: usize) -> String {
    let bytes = unsafe { Vec::from_raw_parts(ptr, len, len) };
    String::from_utf8(bytes).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
}

fn json<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_else(|e| format!("{{\"error\":{:?}}}", e.to_string()))
}

/// Model format version, so the JS side can refuse a mismatched module.
#[unsafe(no_mangle)]
pub extern "C" fn fm_model_version() -> u32 {
    viewmodel::MODEL_VERSION
}

#[unsafe(no_mangle)]
pub extern "C" fn fm_alloc(len: usize) -> *mut u8 {
    let mut v = Vec::<u8>::with_capacity(len.max(1));
    let ptr = v.as_mut_ptr();
    std::mem::forget(v);
    ptr
}

/// Frees a buffer from `fm_alloc` that was not passed to a function.
///
/// # Safety
/// `ptr`/`len` must come from `fm_alloc(len)`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fm_free(ptr: *mut u8, len: usize) {
    drop(unsafe { Vec::from_raw_parts(ptr, 0, len.max(1)) });
}

#[unsafe(no_mangle)]
pub extern "C" fn fm_result_ptr() -> *const u8 {
    RESULT.with(|r| r.borrow().as_ptr())
}

/// The parsed document as JSON.
///
/// # Safety
/// See the protocol in the module docs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fm_parse(src: *mut u8, src_len: usize) -> u32 {
    let source = unsafe { take(src, src_len) };
    set_result(json(&parse(&source)))
}

/// Only the diagnostics, as JSON.
///
/// # Safety
/// See the protocol in the module docs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fm_check(src: *mut u8, src_len: usize) -> u32 {
    let source = unsafe { take(src, src_len) };
    set_result(json(&parse(&source).diagnostics))
}

/// The view model for one file, as JSON.
///
/// # Safety
/// See the protocol in the module docs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fm_model(
    src: *mut u8,
    src_len: usize,
    name: *mut u8,
    name_len: usize,
) -> u32 {
    let (source, name) = unsafe { (take(src, src_len), take(name, name_len)) };
    let doc = parse(&source);
    set_result(json(&viewmodel::build(&doc, &source, &name)))
}

/// The view model for a change from base to head, as JSON.
///
/// # Safety
/// See the protocol in the module docs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fm_diff_model(
    base: *mut u8,
    base_len: usize,
    base_name: *mut u8,
    base_name_len: usize,
    head: *mut u8,
    head_len: usize,
    head_name: *mut u8,
    head_name_len: usize,
) -> u32 {
    let (base, base_name) = unsafe { (take(base, base_len), take(base_name, base_name_len)) };
    let (head, head_name) = unsafe { (take(head, head_len), take(head_name, head_name_len)) };
    let (bd, hd) = (parse(&base), parse(&head));
    set_result(json(&viewmodel::build_diff(
        &bd, &base, &base_name, &hd, &head, &head_name,
    )))
}

/// The text view at a level (like `flowmark view`).
///
/// # Safety
/// See the protocol in the module docs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fm_view(src: *mut u8, src_len: usize, level: u32) -> u32 {
    let source = unsafe { take(src, src_len) };
    set_result(view::render(&parse(&source), &source, level.min(3) as u8))
}
