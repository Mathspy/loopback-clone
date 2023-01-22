//! Feeds back the input stream directly into the output stream.
//!
//! Assumes that the input and output devices can use the same stream configuration and that they
//! support the f32 sample format.
//!
//! Uses a delay of `LATENCY_MS` milliseconds in case the default input and output streams are not
//! precisely synchronised.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::HeapRb;

fn main() -> anyhow::Result<()> {
    let host = cpal::default_host();

    // Find devices.
    let input_device = host
        .input_devices()?
        .find(|device| {
            device
                .name()
                .map(|name| name == "MacBook Pro Microphone")
                .unwrap_or(false)
        })
        .expect("microphone input device exists");
    let output_device = host
        .output_devices()?
        .find(|device| {
            device
                .name()
                .map(|name| name == "BlackHole 16ch")
                .unwrap_or(false)
        })
        .expect("blackhole output device exists");

    println!("Using input device: \"{}\"", input_device.name()?);
    println!("Using output device: \"{}\"", output_device.name()?);

    // We'll try and use the same configuration between streams to keep it simple.
    let config: cpal::StreamConfig = microphone.default_input_config()?.into();

    // The buffer to share samples
    let ring = HeapRb::<f32>::new(
        config
            .sample_rate
            .0
            .try_into()
            .expect("couldn't convert u32 to usize"),
    );
    let (mut producer, mut consumer) = ring.split();

    let input_data_fn = move |data: &[f32], _: &cpal::InputCallbackInfo| {
        let mut output_fell_behind = false;
        for &sample in data {
            if producer.push(sample).is_err() {
                output_fell_behind = true;
            }
        }
        if output_fell_behind {
            eprintln!("output stream fell behind: try increasing latency");
        }
    };

    let output_data_fn = move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
        let mut input_fell_behind = false;
        for sample in data {
            *sample = match consumer.pop() {
                Some(s) => s,
                None => {
                    input_fell_behind = true;
                    0.0
                }
            };
        }
        if input_fell_behind {
            eprintln!("input stream fell behind: try increasing latency");
        }
    };

    // Build streams.
    println!(
        "Attempting to build both streams with f32 samples and `{:?}`.",
        config
    );
    let input_stream = microphone.build_input_stream(&config, input_data_fn, err_fn)?;
    let output_stream = output_device.build_output_stream(&config, output_data_fn, err_fn)?;
    println!("Successfully built streams.");

    // Play the streams.
    println!("Starting the input and output streams",);
    input_stream.play()?;
    output_stream.play()?;

    loop {
        std::thread::park();
    }
}

fn err_fn(err: cpal::StreamError) {
    eprintln!("an error occurred on stream: {}", err);
}
