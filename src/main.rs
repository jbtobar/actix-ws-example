//! Simple echo websocket server.
//!
//! Open `http://localhost:8080/` in browser to test.

// use actix_files::NamedFile;
use actix_web::{
    middleware, rt, web, App, Error, HttpRequest, HttpResponse, HttpServer,
};

mod handler;

// async fn index() -> impl Responder {
//     NamedFile::open_async("./static/index.html").await.unwrap()
// }

/// Handshake and start WebSocket handler with heartbeats.
async fn echo_heartbeat_ws(req: HttpRequest, stream: web::Payload) -> Result<HttpResponse, Error> {
    let (res, session, msg_stream) = actix_ws::handle(&req, stream)?;

    // spawn websocket handler (and don't await it) so that the response is returned immediately
    rt::spawn(handler::echo_heartbeat_ws(session, msg_stream));

    Ok(res)
}

const API_WS_PORT:u16 = 9903;

// note that the `actix` based WebSocket handling would NOT work under `tokio::main`
#[tokio::main(flavor = "current_thread")]
async fn main() -> std::io::Result<()> {
    lib_utils::log_setup();

    log::info!("starting HTTP server at http://localhost:{API_WS_PORT}");

    HttpServer::new(|| {
        App::new()
            // websocket routes
            .service(web::resource("/api/ws").route(web::get().to(echo_heartbeat_ws)))
            // enable logger
            .wrap(middleware::Logger::default())
    })
    // .workers(2)
    .bind(("127.0.0.1", API_WS_PORT))?
    .run()
    .await
}
