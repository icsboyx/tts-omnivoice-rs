//! Small local HTTP example that keeps one model and its voice cache alive.

use std::{
    env,
    error::Error,
    io::{Cursor, Read},
    path::{Path, PathBuf},
};

use serde::Deserialize;
use serde_json::{Value, json};
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};
use uuid::Uuid;
use xtts::{Audio, Error as XttsError, Xtts};

const MAX_REQUEST_BYTES: u64 = 32 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SynthesizeBody {
    id: Uuid,
    text: String,
    speed: Option<f32>,
    pitch: Option<i32>,
}

fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let address = env::var("XTTS_BIND").unwrap_or_else(|_| "127.0.0.1:8002".into());
    let samples_dir = env::var_os("XTTS_SAMPLES_DIR").map_or_else(
        || {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .expect("xtts is inside local_tts")
                .join("xtts/samples")
        },
        PathBuf::from,
    );
    let mut tts = Xtts::new(samples_dir)?;
    let server = Server::http(&address)?;
    println!("Xtts listening on http://{address}");

    for mut request in server.incoming_requests() {
        let response = handle(&mut tts, &mut request);
        if let Err(error) = request.respond(response) {
            eprintln!("Failed to send response: {error}");
        }
    }
    Ok(())
}

fn handle(tts: &mut Xtts, request: &mut Request) -> Response<Cursor<Vec<u8>>> {
    match (request.method(), request.url()) {
        (Method::Get, "/health") => json_response(200, json!({ "status": "ok" })),
        (Method::Get, "/voices") => {
            let mut profiles: Vec<_> = tts.voices().values().map(|voice| &voice.profile).collect();
            profiles.sort_by_key(|profile| profile.id);
            json_response(200, json!(profiles))
        }
        (Method::Post, "/synthesize") => {
            let mut body = Vec::new();
            if let Err(error) = request
                .as_reader()
                .take(MAX_REQUEST_BYTES + 1)
                .read_to_end(&mut body)
            {
                return json_response(400, json!({ "error": error.to_string() }));
            }
            if body.len() as u64 > MAX_REQUEST_BYTES {
                return json_response(413, json!({ "error": "request body is too large" }));
            }
            let input: SynthesizeBody = match serde_json::from_slice(&body) {
                Ok(input) => input,
                Err(error) => return json_response(400, json!({ "error": error.to_string() })),
            };
            match tts.synthesize_voice_with_options(input.id, &input.text, input.speed, input.pitch)
            {
                Ok(audio) => match encode_wav(audio) {
                    Ok(wav) => response(200, "audio/wav", wav),
                    Err(error) => json_response(500, json!({ "error": error.to_string() })),
                },
                Err(error) => {
                    let status = match error {
                        XttsError::UnknownVoice(_) => 404,
                        XttsError::InvalidInput(_) => 400,
                        _ => 500,
                    };
                    json_response(status, json!({ "error": error.to_string() }))
                }
            }
        }
        (Method::Get, _) | (Method::Post, _) => json_response(404, json!({ "error": "not found" })),
        _ => json_response(405, json!({ "error": "method not allowed" })),
    }
}

fn encode_wav(audio: Audio) -> Result<Vec<u8>, hound::Error> {
    let spec = hound::WavSpec {
        channels: audio.channels,
        sample_rate: audio.sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut output = Cursor::new(Vec::new());
    let mut writer = hound::WavWriter::new(&mut output, spec)?;
    for sample in audio.samples {
        writer.write_sample(sample)?;
    }
    writer.finalize()?;
    Ok(output.into_inner())
}

fn json_response(status: u16, value: Value) -> Response<Cursor<Vec<u8>>> {
    response(status, "application/json", value.to_string().into_bytes())
}

fn response(status: u16, content_type: &str, body: Vec<u8>) -> Response<Cursor<Vec<u8>>> {
    Response::from_data(body)
        .with_status_code(StatusCode(status))
        .with_header(Header::from_bytes("Content-Type", content_type).expect("valid header"))
}
