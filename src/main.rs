//! Feeds back the input stream directly into the output stream.
//!
//! Assumes that the input and output devices can use the same stream configuration and that they
//! support the f32 sample format.
//!
//! Uses a delay of `LATENCY_MS` milliseconds in case the default input and output streams are not
//! precisely synchronised.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::{
    ring_buffer::{RbRef, RbWrite},
    HeapRb, Producer,
};

fn create_input_processing_fn<R>(
    mut producer: Producer<f32, R>,
) -> impl FnMut(&[f32], &cpal::InputCallbackInfo)
where
    R: RbRef,
    <R as RbRef>::Rb: RbWrite<f32>,
{
    move |data: &[f32], _: &cpal::InputCallbackInfo| {
        let mut output_fell_behind = false;
        if producer.push_slice(data) != data.len() {
            output_fell_behind = true;
        }
        if output_fell_behind {
            eprintln!("output stream fell behind: try increasing latency");
        }
    }
}

fn main() -> anyhow::Result<()> {
    let host = cpal::default_host();

    // Find devices.
    let microphone = host
        .input_devices()?
        .find(|device| {
            device
                .name()
                .map(|name| name == "MacBook Pro Microphone")
                .unwrap_or(false)
        })
        .expect("microphone input device exists");
    let game_capture = host
        .input_devices()?
        .find(|device| {
            device
                .name()
                .map(|name| name == "Game Capture HD60 X")
                .unwrap_or(false)
        })
        .expect("game capture input device exists");
    let output_device = host
        .output_devices()?
        .find(|device| {
            device
                .name()
                .map(|name| name == "BlackHole 16ch")
                .unwrap_or(false)
        })
        .expect("blackhole output device exists");

    println!("Using input device: \"{}\"", microphone.name()?);
    println!("Using input device: \"{}\"", game_capture.name()?);
    println!("Using output device: \"{}\"", output_device.name()?);

    // We'll try and use the same configuration between streams to keep it simple.
    let config: cpal::StreamConfig = microphone.default_input_config()?.into();

    // The buffer to share samples
    let (producer_mic, mut consumer_mic) = HeapRb::<f32>::new(10_240).split();
    let (producer_capture, mut consumer_capture) = HeapRb::<f32>::new(10_240).split();

    let output_data_fn = move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
        let mut input_fell_behind = false;
        if consumer_mic.len() < data.len() || consumer_capture.len() < data.len() {
            input_fell_behind = true;
        }
        consumer_mic
            .pop_iter()
            .map(Some)
            .chain(std::iter::repeat(None))
            .zip(
                consumer_capture
                    .pop_iter()
                    .map(Some)
                    .chain(std::iter::repeat(None)),
            )
            .zip(data)
            .for_each(|((mic_sample, capture_sample), sample)| {
                *sample = mic_sample.unwrap_or(0.0) + capture_sample.unwrap_or(0.0)
            });
        if input_fell_behind {
            eprintln!("input stream fell behind: try increasing latency");
        }
    };

    // Build streams.
    println!(
        "Attempting to build both streams with f32 samples and `{:?}`.",
        config
    );
    let microphone_stream =
        microphone.build_input_stream(&config, create_input_processing_fn(producer_mic), err_fn)?;
    let game_capture_stream = game_capture.build_input_stream(
        &config,
        create_input_processing_fn(producer_capture),
        err_fn,
    )?;
    let output_stream = output_device.build_output_stream(&config, output_data_fn, err_fn)?;
    println!("Successfully built streams.");

    // Play the streams.
    println!("Starting the input and output streams",);
    microphone_stream.play()?;
    game_capture_stream.play()?;
    output_stream.play()?;

    loop {
        std::thread::park();
    }
}

fn err_fn(err: cpal::StreamError) {
    eprintln!("an error occurred on stream: {}", err);
}
