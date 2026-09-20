extern crate console_error_panic_hook;

use luau_lifter::diagnose_deserialize;
use worker::*;

const MAX_BYTECODE_SIZE: usize = 4 * 1024 * 1024;

fn read_leb128(data: &[u8], pos: &mut usize) -> std::result::Result<usize, String> {
    let mut value = 0usize;
    let mut shift = 0u32;

    for _ in 0..10 {
        if *pos >= data.len() {
            return Err("unexpected EOF while reading LEB128".to_string());
        }
        let byte = data[*pos];
        *pos += 1;
        value |= ((byte & 0x7f) as usize)
            .checked_shl(shift)
            .ok_or_else(|| "LEB128 overflow".to_string())?;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
        shift += 7;
    }

    Err("LEB128 too long".to_string())
}

fn parse_prefix(data: &[u8], through_types: bool, through_protos: bool) -> std::result::Result<String, String> {
    if data.len() < 2 {
        return Err("bytecode shorter than version/types header".to_string());
    }

    let version = data[0];
    let types_version = data[1];
    let mut pos = 2usize;

    let string_count = read_leb128(data, &mut pos)?;
    if string_count > 1_000_000 {
        return Err(format!("unreasonable string_count={string_count}"));
    }

    let mut string_bytes = 0usize;
    for index in 0..string_count {
        let len = read_leb128(data, &mut pos)?;
        let end = pos.checked_add(len).ok_or_else(|| "string offset overflow".to_string())?;
        if end > data.len() {
            return Err(format!("string {index} exceeds payload: len={len} pos={pos}"));
        }
        string_bytes = string_bytes.saturating_add(len);
        pos = end;
    }

    if !through_types {
        return Ok(format!(
            "NEY_DIAG_STRINGS_OK version={version} types_version={types_version} strings={string_count} string_bytes={string_bytes} pos={pos} remaining={}",
            data.len().saturating_sub(pos)
        ));
    }

    let mut type_entries = 0usize;
    if types_version == 3 {
        loop {
            if pos >= data.len() {
                return Err("EOF before types terminator".to_string());
            }
            if data[pos] == 0 {
                pos += 1;
                break;
            }
            let _ = read_leb128(data, &mut pos)?;
            type_entries += 1;
            if type_entries > data.len() {
                return Err("types section iteration guard triggered".to_string());
            }
        }
    }

    if !through_protos {
        return Ok(format!(
            "NEY_DIAG_TYPES_OK version={version} types_version={types_version} strings={string_count} type_entries={type_entries} pos={pos} remaining={}",
            data.len().saturating_sub(pos)
        ));
    }

    let function_count = read_leb128(data, &mut pos)?;
    if function_count > 1_000_000 {
        return Err(format!("unreasonable function_count={function_count}"));
    }

    let mut total_proto_bytes = 0usize;
    let mut max_proto_size = 0usize;
    for index in 0..function_count {
        let proto_size = read_leb128(data, &mut pos)?;
        let end = pos.checked_add(proto_size).ok_or_else(|| "proto offset overflow".to_string())?;
        if end > data.len() {
            return Err(format!("proto {index} exceeds payload: size={proto_size} pos={pos}"));
        }
        total_proto_bytes = total_proto_bytes.saturating_add(proto_size);
        max_proto_size = max_proto_size.max(proto_size);
        pos = end;
    }

    let main = read_leb128(data, &mut pos)?;
    Ok(format!(
        "NEY_DIAG_PROTOS_OK version={version} types_version={types_version} strings={string_count} type_entries={type_entries} functions={function_count} total_proto_bytes={total_proto_bytes} max_proto_size={max_proto_size} main={main} pos={pos} remaining={}",
        data.len().saturating_sub(pos)
    ))
}

async fn body_bytes(mut req: Request) -> Result<Vec<u8>> {
    let bytecode = req.bytes().await?;
    if bytecode.is_empty() {
        return Err(Error::RustError("empty bytecode".to_string()));
    }
    if bytecode.len() > MAX_BYTECODE_SIZE {
        return Err(Error::RustError("bytecode too large".to_string()));
    }
    Ok(bytecode)
}

#[event(fetch, respond_with_errors)]
pub async fn main(req: Request, env: Env, _ctx: worker::Context) -> Result<Response> {
    console_error_panic_hook::set_once();

    Router::new()
        .get("/health", |_req, _ctx| Response::ok("neyy-luau-decompiler staged diagnostic"))
        .post_async("/diag/strings", |req, _ctx| async move {
            let bytecode = body_bytes(req).await?;
            match parse_prefix(&bytecode, false, false) {
                Ok(s) => Response::ok(s),
                Err(e) => Response::error(format!("NEY_DIAG_STRINGS_ERR {e}"), 422),
            }
        })
        .post_async("/diag/types", |req, _ctx| async move {
            let bytecode = body_bytes(req).await?;
            match parse_prefix(&bytecode, true, false) {
                Ok(s) => Response::ok(s),
                Err(e) => Response::error(format!("NEY_DIAG_TYPES_ERR {e}"), 422),
            }
        })
        .post_async("/diag/protos", |req, _ctx| async move {
            let bytecode = body_bytes(req).await?;
            match parse_prefix(&bytecode, true, true) {
                Ok(s) => Response::ok(s),
                Err(e) => Response::error(format!("NEY_DIAG_PROTOS_ERR {e}"), 422),
            }
        })
        .post_async("/decompile", |req, _ctx| async move {
            let bytecode = body_bytes(req).await?;
            Response::ok(diagnose_deserialize(&bytecode, 203))
        })
        .run(req, env)
        .await
}
