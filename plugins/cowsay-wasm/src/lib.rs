#![no_std]

extern crate alloc;

use alloc::alloc::{GlobalAlloc, Layout};
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
use core::panic::PanicInfo;
use core::sync::atomic::{AtomicUsize, Ordering};
use core::str;

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    loop {}
}

struct BumpAlloc {
    heap: [u8; 65536],
    used: AtomicUsize,
}

unsafe impl GlobalAlloc for BumpAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let align = layout.align();
        let size = layout.size();
        loop {
            let current = self.used.load(Ordering::SeqCst);
            let aligned = (current + align - 1) & !(align - 1);
            if aligned + size <= self.heap.len() {
                if self.used.compare_exchange(current, aligned + size, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
                    return self.heap.as_ptr().add(aligned) as *mut u8;
                }
            } else {
                return core::ptr::null_mut();
            }
        }
    }
    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
}

#[global_allocator]
static ALLOCATOR: BumpAlloc = BumpAlloc {
    heap: [0u8; 65536],
    used: AtomicUsize::new(0),
};

fn read_from_memory<'a>(ptr: i32, len: i32) -> &'a str {
    unsafe {
        let slice = core::slice::from_raw_parts(ptr as *const u8, len as usize);
        str::from_utf8_unchecked(slice)
    }
}

fn extract_json_string(json: &str, key: &str) -> Option<String> {
    let search = alloc::format!("\"{}\"", key);
    let key_start = json.find(&search)?;
    let after_key = &json[key_start + search.len()..];
    let colon = after_key.find(':')?;
    let after_colon = after_key[colon + 1..].trim_start();
    if after_colon.starts_with('"') {
        let content = &after_colon[1..];
        let mut result = String::new();
        let mut chars = content.chars();
        loop {
            match chars.next() {
                None => break,
                Some('"') => break,
                Some('\\') => {
                    match chars.next() {
                        Some('"') => result.push('"'),
                        Some('n') => result.push('\n'),
                        Some('\\') => result.push('\\'),
                        Some(c) => { result.push('\\'); result.push(c); }
                        None => break,
                    }
                }
                Some(c) => result.push(c),
            }
        }
        Some(result)
    } else {
        None
    }
}

fn cowsay(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let max_len = lines.iter().map(|l| l.len()).max().unwrap_or(0);
    let mut result = String::new();
    result.push(' ');
    for _ in 0..max_len + 2 {
        result.push('_');
    }
    result.push('\n');
    for line in &lines {
        result.push_str("| ");
        result.push_str(line);
        for _ in 0..(max_len - line.len()) {
            result.push(' ');
        }
        result.push_str(" |\n");
    }
    result.push(' ');
    for _ in 0..max_len + 2 {
        result.push('-');
    }
    result.push('\n');
    result.push_str("        \\   ^__^\n");
    result.push_str("         \\  (oo)\\_______\n");
    result.push_str("            (__)\\       )\\/\\\n");
    result.push_str("                ||----w |\n");
    result.push_str("                ||     ||\n");
    result
}

fn escape_json(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out
}

#[no_mangle]
pub extern "C" fn alloc(len: i32) -> i32 {
    if len <= 0 {
        return 0;
    }
    let layout = Layout::from_size_align(len as usize, 1).unwrap();
    let ptr = unsafe { ALLOCATOR.alloc(layout) };
    ptr as i32
}

#[no_mangle]
pub extern "C" fn dealloc(ptr: i32, len: i32) {
    if ptr != 0 && len > 0 {
        let layout = Layout::from_size_align(len as usize, 1).unwrap();
        unsafe { ALLOCATOR.dealloc(ptr as *mut u8, layout); }
    }
}

#[no_mangle]
pub extern "C" fn plugin_name() -> i32 {
    let s = "cowsay-wasm\0";
    let bytes = s.as_bytes();
    let layout = Layout::from_size_align(bytes.len(), 1).unwrap();
    let ptr = unsafe { ALLOCATOR.alloc(layout) };
    unsafe {
        core::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
    }
    ptr as i32
}

#[no_mangle]
pub extern "C" fn plugin_version() -> i32 {
    let s = "0.1.0\0";
    let bytes = s.as_bytes();
    let layout = Layout::from_size_align(bytes.len(), 1).unwrap();
    let ptr = unsafe { ALLOCATOR.alloc(layout) };
    unsafe {
        core::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
    }
    ptr as i32
}

#[no_mangle]
pub extern "C" fn plugin_describe() -> i32 {
    let json = "[{\"name\":\"cowsay\",\"description\":\"In ra mot con bo noi (cowsay) voi noi dung cho truoc\",\"input_schema\":{\"type\":\"object\",\"properties\":{\"message\":{\"type\":\"string\",\"description\":\"Noi dung de con bo noi\"}},\"required\":[\"message\"]}}]\0";
    let bytes = json.as_bytes();
    let layout = Layout::from_size_align(bytes.len(), 1).unwrap();
    let ptr = unsafe { ALLOCATOR.alloc(layout) };
    unsafe {
        core::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
    }
    ptr as i32
}

#[no_mangle]
pub extern "C" fn plugin_execute(
    name_ptr: i32, name_len: i32,
    args_ptr: i32, args_len: i32,
) -> i32 {
    let tool_name = read_from_memory(name_ptr, name_len);
    let args_json = read_from_memory(args_ptr, args_len);

    let message = if tool_name == "cowsay" {
        extract_json_string(args_json, "message").unwrap_or_else(|| "Moo!".to_string())
    } else {
        alloc::format!("Unknown tool: {}", tool_name)
    };

    let output = cowsay(&message);
    let escaped = escape_json(&output);
    let result = alloc::format!("{{\"content\":[{{\"type\":\"text\",\"text\":\"{}\"}}],\"isError\":false}}", escaped);
    let len = result.len();
    let layout = Layout::from_size_align(len + 1, 1).unwrap();
    let ptr = unsafe { ALLOCATOR.alloc(layout) };
    unsafe {
        core::ptr::copy_nonoverlapping(result.as_ptr(), ptr, len);
        *ptr.add(len) = 0;
    }
    ptr as i32
}
