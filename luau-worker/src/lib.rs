extern crate console_error_panic_hook;

use luau_lifter::diagnose_deserialize;
use worker::*;

const MAX_BYTECODE_SIZE: usize = 4 * 1024 * 1024;

#[event(fetch, respond_with_errors)]
pub async fn main(req: Request, env: Env, _ctx: worker::Context) -> Result<Response> {
    console_error_panic_hook::set_once();

    Router::new()
        .get("/health", |_req, _ctx| Response::ok("neyy-luau-decompiler diagnostic deserialize"))
        .post_async("/decompile", |mut req, _ctx| async move {
            let bytecode = req.bytes().await?;

            if bytecode.is_empty() {
                return Response::error("empty bytecode", 400);
            }

            if bytecode.len() > MAX_BYTECODE_SIZE {
                return Response::error("bytecode too large", 413);
            }

            Response::ok(diagnose_deserialize(&bytecode, 203))
        })
        .run(req, env)
        .await
}
