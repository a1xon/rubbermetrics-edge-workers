use worker::*;

#[event(fetch)]
pub async fn main(_req: Request, _env: Env, _ctx: worker::Context) -> Result<Response> {
    Response::ok("Search Worker Placeholder")
}
