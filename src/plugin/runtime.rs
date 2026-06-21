use std::path::Path;
use wasmtime::*;
use serde_json::Value;
use crate::mcp::types::{ToolDefinition, ToolResult};

pub struct WasmPlugin {
    #[allow(dead_code)]
    pub name: String,
    #[allow(dead_code)]
    pub version: String,
    pub tools: Vec<ToolDefinition>,
    engine: Engine,
    module: Module,
    linker: Linker<()>,
}

fn read_string(store: &Store<()>, memory: &Memory, ptr: i32) -> Result<(String, i32), String> {
    read_length_prefixed_string(store, memory, ptr)
        .or_else(|_| read_null_terminated_string(store, memory, ptr))
}

fn read_length_prefixed_string(store: &Store<()>, memory: &Memory, ptr: i32) -> Result<(String, i32), String> {
    let mut len_bytes = [0u8; 4];
    memory.read(store, ptr as usize, &mut len_bytes)
        .map_err(|e| format!("memory read error: {}", e))?;
    let len = u32::from_le_bytes(len_bytes) as i32;
    
    if len < 0 || len > 1024 * 1024 {
        return Err("Invalid length prefix".into());
    }
    
    let mut bytes = vec![0u8; len as usize];
    memory.read(store, (ptr + 4) as usize, &mut bytes)
        .map_err(|e| format!("memory read error: {}", e))?;
    
    let string = String::from_utf8(bytes).map_err(|e| format!("UTF-8 error: {}", e))?;
    Ok((string, len + 4))
}

fn read_null_terminated_string(store: &Store<()>, memory: &Memory, ptr: i32) -> Result<(String, i32), String> {
    let mut offset = ptr as usize;
    let mut bytes = Vec::new();
    loop {
        let mut byte = [0u8; 1];
        memory.read(store, offset, &mut byte).map_err(|e| format!("memory read error: {}", e))?;
        if byte[0] == 0 {
            break;
        }
        bytes.push(byte[0]);
        offset += 1;
        if bytes.len() > 1024 * 1024 {
            return Err("String too long".into());
        }
    }
    let string = String::from_utf8(bytes).map_err(|e| format!("UTF-8 error: {}", e))?;
    let total_len = (offset - ptr as usize) as i32 + 1;
    Ok((string, total_len))
}

impl WasmPlugin {
    pub fn load(path: &Path) -> Result<Self, String> {
        let engine = Engine::default();
        let module = Module::from_file(&engine, path)
            .map_err(|e| format!("module load error: {}", e))?;

        let mut linker = Linker::new(&engine);

        linker.func_wrap("env", "log", |msg_ptr: i32, msg_len: i32| {
            eprintln!("[wasm-plugin-log] <raw {} bytes at ptr {}>", msg_len, msg_ptr);
        }).map_err(|e| format!("linker error: {}", e))?;

        let mut store = Store::new(&engine, ());
        let instance = linker.instantiate(&mut store, &module)
            .map_err(|e| format!("instantiate error: {}", e))?;

        let memory = instance.get_memory(&mut store, "memory")
            .ok_or_else(|| "WASM plugin must export 'memory'".to_string())?;

            let name = {
                let name_fn = instance.get_typed_func::<(), i32>(&mut store, "plugin_name")
                    .map_err(|e| format!("plugin_name missing: {}", e))?;
                let ptr = name_fn.call(&mut store, ()).map_err(|e| format!("plugin_name call error: {}", e))?;
                read_string(&store, &memory, ptr)?.0
            };

            let version = {
                let ver_fn = instance.get_typed_func::<(), i32>(&mut store, "plugin_version")
                    .map_err(|e| format!("plugin_version missing: {}", e))?;
                let ptr = ver_fn.call(&mut store, ()).map_err(|e| format!("plugin_version call error: {}", e))?;
                read_string(&store, &memory, ptr)?.0
            };

            let tools = {
                let desc_fn = instance.get_typed_func::<(), i32>(&mut store, "plugin_describe")
                    .map_err(|e| format!("plugin_describe missing: {}", e))?;
                let ptr = desc_fn.call(&mut store, ()).map_err(|e| format!("plugin_describe call error: {}", e))?;
                let json_str = read_string(&store, &memory, ptr)?.0;
            serde_json::from_str::<Vec<ToolDefinition>>(&json_str)
                .map_err(|e| format!("invalid tool definitions JSON: {}", e))?
        };

        Ok(Self {
            name,
            version,
            tools,
            engine,
            module,
            linker,
        })
    }

    pub fn execute(&self, tool_name: &str, args: Value) -> Result<ToolResult, String> {
        let mut store = Store::new(&self.engine, ());

        let instance = self.linker.instantiate(&mut store, &self.module)
            .map_err(|e| format!("re-instantiate error: {}", e))?;

        let memory = instance.get_memory(&mut store, "memory")
            .ok_or_else(|| "WASM plugin must export 'memory'".to_string())?;

        let name_bytes = tool_name.as_bytes();
        let args_json = serde_json::to_string(&args).map_err(|e| e.to_string())?;
        let args_bytes = args_json.as_bytes();

        let alloc_fn = instance.get_typed_func::<(i32,), i32>(&mut store, "alloc")
            .map_err(|e| format!("alloc missing: {}", e))?;

        let dealloc_fn = instance.get_typed_func::<(i32, i32), ()>(&mut store, "dealloc")
            .map_err(|e| format!("dealloc missing: {}", e))?;

        let name_ptr = {
            let len = name_bytes.len() as i32;
            let ptr = alloc_fn.call(&mut store, (len,)).map_err(|e| format!("alloc call error: {}", e))?;
            memory.write(&mut store, ptr as usize, name_bytes).map_err(|e| format!("memory write error: {}", e))?;
            ptr
        };

        let args_ptr = {
            let len = args_bytes.len() as i32;
            let ptr = alloc_fn.call(&mut store, (len,)).map_err(|e| format!("alloc call error: {}", e))?;
            memory.write(&mut store, ptr as usize, args_bytes).map_err(|e| format!("memory write error: {}", e))?;
            ptr
        };

        let execute_fn = instance.get_typed_func::<(i32, i32, i32, i32), i32>(&mut store, "plugin_execute")
            .map_err(|e| format!("plugin_execute missing: {}", e))?;

        let result_ptr = execute_fn.call(&mut store, (name_ptr, name_bytes.len() as i32, args_ptr, args_bytes.len() as i32))
            .map_err(|e| format!("plugin_execute call error: {}", e))?;

        let (result_str, result_len) = read_string(&store, &memory, result_ptr)?;

        dealloc_fn.call(&mut store, (name_ptr, name_bytes.len() as i32)).ok();
        dealloc_fn.call(&mut store, (args_ptr, args_bytes.len() as i32)).ok();
        dealloc_fn.call(&mut store, (result_ptr, result_len)).ok();

        let result: ToolResult = serde_json::from_str(&result_str)
            .map_err(|e| format!("invalid result JSON from plugin: {}", e))?;

        Ok(result)
    }
}
