//! Process (stereo) input and play the result (in stereo).
mod reverb;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};
use crossbeam_channel::{bounded, Receiver, Sender};
use reverb::Freeverb;

fn main() {
    let (sender, receiver) = bounded(4096);

    let host = cpal::default_host();

    let in_device = host.default_input_device().unwrap();
    let in_config = in_device.default_input_config().unwrap();
    match in_config.sample_format() {
        cpal::SampleFormat::F32 => run_in::<f32>(&in_device, &in_config.into(), sender),
        cpal::SampleFormat::I16 => run_in::<i16>(&in_device, &in_config.into(), sender),
        cpal::SampleFormat::U16 => run_in::<u16>(&in_device, &in_config.into(), sender),
        format => eprintln!("Unsupported sample format: {}", format),
    }

    let out_device = host.default_output_device().unwrap();
    let out_config = out_device.default_output_config().unwrap();
    match out_config.sample_format() {
        cpal::SampleFormat::F32 => run_out::<f32>(&out_device, &out_config.into(), receiver),
        cpal::SampleFormat::I16 => run_out::<i16>(&out_device, &out_config.into(), receiver),
        cpal::SampleFormat::U16 => run_out::<u16>(&out_device, &out_config.into(), receiver),
        format => eprintln!("Unsupported sample format: {}", format),
    }
    println!("Processing stereo input to stereo output.");
    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

fn run_in<T>(device: &cpal::Device, config: &cpal::StreamConfig, sender: Sender<(f32, f32)>)
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let channels = config.channels as usize;
    let err_fn = |err| eprintln!("an error occurred on stream: {}", err);
    let stream = device.build_input_stream(
        config,
        move |data: &[T], _: &cpal::InputCallbackInfo| read_data(data, channels, sender.clone()),
        err_fn,
        None,
    );
    if let Ok(stream) = stream {
        if let Ok(()) = stream.play() {
            std::mem::forget(stream);
        }
    }
    println!("Input stream built.");
}

fn read_data<T>(input: &[T], channels: usize, sender: Sender<(f32, f32)>)
where
    T: SizedSample,
    f32: FromSample<T>,
{
    for frame in input.chunks(channels) {
        let mut left = 0.0;
        let mut right = 0.0;
        for (channel, sample) in frame.iter().enumerate() {
            if channel & 1 == 0 {
                left = sample.to_sample::<f32>();
            } else {
                right = sample.to_sample::<f32>();
            }
        }
        if let Ok(()) = sender.try_send((left, right)) {}
    }
}

fn run_out<T>(device: &cpal::Device, config: &cpal::StreamConfig, receiver: Receiver<(f32, f32)>)
where
    T: SizedSample + FromSample<f32>,
{
    let channels = config.channels as usize;

    let mut freeverb = Freeverb::new(44_100);

    let mut next_value = move || {
        let sample = receiver.try_recv().unwrap_or((0.0, 0.0));
        let out = freeverb.tick(sample);
        out
    };

    let err_fn = |err| eprintln!("An error occurred on stream: {}", err);
    let stream = device.build_output_stream(
        config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            write_data(data, channels, &mut next_value)
        },
        err_fn,
        None,
    );
    if let Ok(stream) = stream {
        if let Ok(()) = stream.play() {
            std::mem::forget(stream);
        }
    }
}

fn write_data<T>(output: &mut [T], channels: usize, next_sample: &mut dyn FnMut() -> (f32, f32))
where
    T: SizedSample + FromSample<f32>,
{
    for frame in output.chunks_mut(channels) {
        let sample = next_sample();
        let left = T::from_sample(sample.0);
        let right = T::from_sample(sample.1);

        for (channel, sample) in frame.iter_mut().enumerate() {
            if channel & 1 == 0 {
                *sample = left;
            } else {
                *sample = right;
            }
        }
    }
}