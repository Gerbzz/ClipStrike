#![cfg(feature = "gstreamer-backend")]

use clipstrike_core::project::Project;
use clipstrike_ui::ui::ClipStrikeApp;
use egui_kittest::{Harness, kittest::Queryable};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::{Duration, Instant};

struct PipelineGuard(gst::Pipeline);

impl Drop for PipelineGuard {
    fn drop(&mut self) {
        let _ = self.0.set_state(gst::State::Null);
    }
}

fn configure_gstreamer_runtime() {
    let gst_root = r"C:\Program Files\gstreamer\1.0\msvc_x86_64";
    unsafe {
        std::env::set_var(
            "GST_PLUGIN_SCANNER",
            format!(r"{gst_root}\libexec\gstreamer-1.0\gst-plugin-scanner.exe"),
        );
        std::env::set_var(
            "GST_PLUGIN_SYSTEM_PATH_1_0",
            format!(r"{gst_root}\lib\gstreamer-1.0"),
        );
        std::env::set_var(
            "GST_PLUGIN_PATH_1_0",
            format!(r"{gst_root}\lib\gstreamer-1.0"),
        );
    }
}

#[test]
fn headless_play_button_and_gstreamer_pipeline_streams_buffers() {
    configure_gstreamer_runtime();
    gst::init().expect("gstreamer should initialize");

    let project = Project::new("E2E Harness", 1920, 1080, 30.0);
    let app = ClipStrikeApp::test_harness(project);
    let mut harness = Harness::new_ui_state(|ui, app| app.render_transport_test_ui(ui), app);

    let initial = harness.state().test_snapshot();
    assert_eq!(initial["is_playing"].as_bool(), Some(false));

    harness.get_by_label("Play").click();
    harness.run();

    let after_click = harness.state().test_snapshot();
    assert_eq!(after_click["is_playing"].as_bool(), Some(true));

    let (pipeline, sample_count) = build_headless_pipeline();
    let _guard = PipelineGuard(pipeline.clone());

    pipeline
        .set_state(gst::State::Playing)
        .expect("pipeline should transition to Playing");

    let current_state = pipeline.state(Some(gst::ClockTime::from_seconds(5))).1;
    assert_eq!(current_state, gst::State::Playing);

    let bus = pipeline.bus().expect("pipeline should expose a bus");
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut saw_sample = false;

    while Instant::now() < deadline {
        if let Some(msg) = bus.timed_pop_filtered(
            Some(gst::ClockTime::from_mseconds(100)),
            &[gst::MessageType::Error, gst::MessageType::Eos],
        ) {
            match msg.view() {
                gst::MessageView::Error(err) => {
                    panic!(
                        "gstreamer pipeline error: {} ({:?})",
                        err.error(),
                        err.debug()
                    );
                }
                gst::MessageView::Eos(_) => break,
                _ => {}
            }
        }

        if sample_count.load(Ordering::SeqCst) > 0 {
            saw_sample = true;
            break;
        }
    }

    assert!(
        saw_sample,
        "expected appsink to receive at least one buffer"
    );
}

fn build_headless_pipeline() -> (gst::Pipeline, Arc<AtomicUsize>) {
    let pipeline = gst::Pipeline::new();
    let src = gst::ElementFactory::make("videotestsrc")
        .property("is-live", true)
        .property("num-buffers", 8i32)
        .build()
        .expect("videotestsrc should be available");
    let convert = gst::ElementFactory::make("videoconvert")
        .build()
        .expect("videoconvert should be available");
    let sink = gst_app::AppSink::builder()
        .name("headless_probe_sink")
        .sync(false)
        .max_buffers(4)
        .drop(true)
        .build();

    pipeline
        .add_many([&src, &convert, sink.upcast_ref()])
        .expect("pipeline should accept all elements");
    gst::Element::link_many([&src, &convert, sink.upcast_ref()]).expect("pipeline should link");

    let sample_count = Arc::new(AtomicUsize::new(0));
    let sample_count_for_cb = Arc::clone(&sample_count);
    sink.set_callbacks(
        gst_app::AppSinkCallbacks::builder()
            .new_sample(move |sink| {
                let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                let buffer = sample.buffer().ok_or(gst::FlowError::Error)?;
                let _map = buffer.map_readable().map_err(|_| gst::FlowError::Error)?;
                sample_count_for_cb.fetch_add(1, Ordering::SeqCst);
                Ok(gst::FlowSuccess::Ok)
            })
            .build(),
    );

    (pipeline, sample_count)
}
