use super::*;
use gpui_kit::{ScrollDelta, TestAppContext, test::TestWindowExt};

#[gpui_kit::test]
fn statistics_follow_draft_and_ignore_stale_results_without_saving(cx: &mut TestAppContext) {
    let settings = open(cx);
    cx.update_window(settings.handle, |_, window, cx| {
        let original = settings.statistics_query(false, cx).unwrap();
        settings.statistics_queried(original.clone(), cx);
        assert!(settings.statistics_query(false, cx).is_none());
        settings.statistics(original.clone(), Ok(vec![]), cx);
        assert!(
            settings
                .view
                .read(cx)
                .statistics_report(cx)
                .contains("No statistics")
        );
        let prompt_id = settings.view.read(cx).prompt.entity_id();
        settings.view.update(cx, |view, cx| {
            view.model.update(cx, |select, cx| {
                select.set_selected_index(Some(IndexPath::new(1)), window, cx)
            });
            view.beam
                .update(cx, |slider, cx| slider.set_value(7., window, cx));
            view.device.update(cx, |select, cx| {
                select.set_selected_index(Some(IndexPath::new(2)), window, cx)
            });
            view.language.update(cx, |select, cx| {
                select.set_selected_index(Some(IndexPath::new(2)), window, cx)
            });
            view.draft.enable_vad = !original.vad;
        });
        let changed = settings.statistics_query(false, cx).unwrap();
        assert_ne!(changed.model, original.model);
        assert_eq!(changed.beam_size, 7);
        assert_eq!(changed.device, Device::Cpu);
        assert_eq!(changed.language, Language::Ua);
        assert_ne!(changed.vad, original.vad);
        settings.statistics(original, Err("stale error".into()), cx);
        assert!(
            settings
                .view
                .read(cx)
                .statistics_report(cx)
                .contains("Loading statistics")
        );
        settings.statistics(
            changed.clone(),
            Ok(vec![StatisticsSummary {
                device: "cpu".into(),
                count: 3,
                average_ms: 123.4,
                average_words: 12.6,
                average_audio_seconds: 2.7,
                ms_per_audio_second: 61.7,
                average_vad_ms: Some(4.6),
                vad_count: 3,
                average_preparation_ms: Some(15.4),
                preparation_count: 3,
                model_load_count: 1,
                average_model_loading_ms: Some(1000.2),
            }]),
            cx,
        );
        let report = settings.view.read(cx).statistics_report(cx);
        assert!(report.contains("CPU Transcriptions: 3"));
        assert!(report.contains("Average transcription: 123 ms"));
        assert!(report.contains("Average phrase: 13 words"));
        assert!(report.contains("Average input audio: 3 s"));
        assert!(report.contains("Processing per audio second: 61.7 ms/s"));
        assert!(report.contains("Average audio preparation: 15 ms"));
        assert!(report.contains(if changed.vad {
            "Average VAD: 5 ms"
        } else {
            "Average VAD: Off"
        }));
        settings
            .view
            .update(cx, |view, _| view.draft.keep_in_memory = true);
        assert!(
            !settings
                .view
                .read(cx)
                .statistics_report(cx)
                .contains("Average model loading")
        );
        assert!(
            !settings
                .view
                .read(cx)
                .statistics_report(cx)
                .contains("Model load measurements")
        );
        settings
            .view
            .update(cx, |view, _| view.draft.keep_in_memory = false);
        let report = settings.view.read(cx).statistics_report(cx);
        assert!(report.contains("Model load measurements: 1"));
        assert!(report.contains("Average model loading: 1000 ms"));
        assert_eq!(settings.statistics_query(true, cx), Some(changed));
        assert!(settings.take_save(cx).is_none());
        assert_eq!(settings.view.read(cx).prompt.entity_id(), prompt_id);
    })
    .unwrap();
}

fn open(cx: &mut TestAppContext) -> SettingsWindow {
    cx.update(|cx| {
        gpui_kit::init(cx);
        super::super::settings_theme::install(cx).unwrap();
        SettingsWindow::open(
            Settings::initial().unwrap(),
            41,
            "%TEMP%/VoicePaste/voicepaste.log",
            cx,
        )
        .unwrap()
    })
}

#[test]
fn model_size_uses_only_selected_model_bin_without_loading() {
    assert_eq!(model_size_label("large-v2"), "Model size: 2.87 GiB");
    assert_eq!(model_size_label("large-v3"), "Model size: 2.88 GiB");
    assert_eq!(model_size_label("large-v3-turbo"), "Model size: 1.51 GiB");
    assert_eq!(model_size_label("unknown"), "Model size: unavailable");
}

#[test]
fn phase_times_distinguish_no_history_from_a_real_zero_measurement() {
    assert_eq!(phase_time(None), "No measurements yet");
    assert_eq!(phase_time(Some(0.0)), "0 ms");
    assert_eq!(phase_time(Some(123.6)), "124 ms");
}

#[gpui_kit::test]
fn statistics_reset_waits_for_completion_and_preserves_settings_draft(cx: &mut TestAppContext) {
    let settings = open(cx);
    cx.update_window(settings.handle, |_, _, cx| {
        let filter = settings.statistics_query(false, cx).unwrap();
        settings.statistics_queried(filter.clone(), cx);
        settings.statistics(
            filter.clone(),
            Ok(vec![StatisticsSummary {
                device: "cpu".into(),
                count: 2,
                average_ms: 42.0,
                ..StatisticsSummary::default()
            }]),
            cx,
        );
        let draft = settings.view.read(cx).draft.clone();
        let prompt_id = settings.view.read(cx).prompt.entity_id();
        settings
            .view
            .update(cx, |view, cx| view.reset_statistics(cx));
        assert!(settings.take_statistics_reset(cx));
        assert!(!settings.take_statistics_reset(cx));
        assert!(settings.statistics_query(true, cx).is_none());
        settings.statistics(filter.clone(), Err("stale query".into()), cx);
        assert!(
            settings
                .view
                .read(cx)
                .statistics_report(cx)
                .contains("42 ms")
        );
        settings.statistics_reset(&Err("database locked".into()), cx);
        assert!(!settings.view.read(cx).statistics_reset_pending);
        assert_eq!(
            settings.view.read(cx).statistics_reset_error,
            "database locked"
        );
        assert!(
            settings
                .view
                .read(cx)
                .statistics_report(cx)
                .contains("42 ms")
        );
        settings
            .view
            .update(cx, |view, cx| view.reset_statistics(cx));
        assert!(settings.take_statistics_reset(cx));
        settings.statistics_reset(&Ok(()), cx);
        assert!(settings.view.read(cx).statistics_reset_error.is_empty());
        assert_eq!(settings.statistics_query(false, cx), Some(filter.clone()));
        settings.statistics(filter, Ok(vec![]), cx);
        assert!(
            settings
                .view
                .read(cx)
                .statistics_report(cx)
                .contains("No statistics")
        );
        assert_eq!(settings.view.read(cx).draft, draft);
        assert_eq!(settings.view.read(cx).prompt.entity_id(), prompt_id);
        assert!(settings.take_save(cx).is_none());
    })
    .unwrap();
}

#[gpui_kit::test]
fn apply_keeps_inputs_and_window_open_then_ok_closes_on_success(cx: &mut TestAppContext) {
    let settings = open(cx);
    let applied = cx
        .update_window(settings.handle, |_, window, cx| {
            settings.view.update(cx, |view, _| {
                view.draft.start_at_login = true;
            });
            window.render_frame(cx);
            window.click("apply", cx);
            let applied = settings.take_save(cx).unwrap();
            assert!(applied.start_at_login);
            let prompt_id = settings.view.read(cx).prompt.entity_id();
            settings.saved(&Ok(applied.clone()), cx);
            assert!(!settings.is_closed());
            assert!(!settings.view.read(cx).busy);
            assert_eq!(settings.view.read(cx).prompt.entity_id(), prompt_id);
            assert!(settings.take_save(cx).is_none());
            // Further edits are an independent draft; failed Apply keeps them.
            settings.view.update(cx, |view, _| {
                view.draft.start_at_login = false;
            });
            window.render_frame(cx);
            window.click("apply", cx);
            assert!(!settings.take_save(cx).unwrap().start_at_login);
            settings.saved(&Err("Write failed".into()), cx);
            assert!(!settings.is_closed());
            assert!(!settings.view.read(cx).busy);
            assert!(!settings.view.read(cx).draft.start_at_login);
            window.render_frame(cx);
            window.click("ok", cx);
            settings.take_save(cx).unwrap()
        })
        .unwrap();
    cx.update(|cx| settings.saved(&Ok(applied), cx));
    assert!(settings.is_closed());
}

#[gpui_kit::test]
fn cancel_after_apply_does_not_submit_later_edits(cx: &mut TestAppContext) {
    let settings = open(cx);
    cx.update_window(settings.handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("apply", cx);
        let applied = settings.take_save(cx).unwrap();
        settings.saved(&Ok(applied), cx);
        settings
            .view
            .update(cx, |view, _| view.draft.start_at_login = true);
        window.render_frame(cx);
        window.click("cancel", cx);
        assert!(settings.is_closed());
        assert!(settings.take_save(cx).is_none());
    })
    .unwrap();
}

#[gpui_kit::test]
fn unchanged_save_preserves_all_non_geometry_settings(cx: &mut TestAppContext) {
    let settings = open(cx);
    cx.update_window(settings.handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("ok", cx);
        let mut request = settings.take_save(cx).expect("Save submits draft");
        let initial = Settings::initial().unwrap();
        request.settings_window_width = initial.settings_window_width;
        request.settings_window_height = initial.settings_window_height;
        request.settings_window_left = initial.settings_window_left;
        request.settings_window_top = initial.settings_window_top;
        assert_eq!(
            request, initial,
            "Includes the exact bilingual custom prompt and policies"
        );
        assert!(settings.view.read(cx).busy);
        assert!(settings.take_save(cx).is_none());
    })
    .unwrap();
}

#[gpui_kit::test]
fn appearance_uses_one_slider_across_modes_and_preserves_draft_until_save(cx: &mut TestAppContext) {
    let settings = open(cx);
    let saved = cx
        .update_window(settings.handle, |_, window, cx| {
            settings.view.update(cx, |view, cx| {
                view.page = SettingsPage::Appearance;
                view.transparency
                    .update(cx, |slider, cx| slider.set_value(73., window, cx));
            });
            window.render_frame(cx);
            window.click("acrylic", cx);
            assert_eq!(settings.view.read(cx).draft.overlay_acrylic, Some(false));
            let checkbox = window.find("overlay-acrylic").bounds();
            let slider = window.find("overlay-transparency").bounds();
            assert!(checkbox.bottom() <= slider.top());
            let slider_id = settings.view.read(cx).transparency.entity_id();
            for acrylic in [true, false, true] {
                settings
                    .view
                    .update(cx, |view, _| view.draft.overlay_acrylic = Some(acrylic));
                settings.memory("Main application: 50 MiB", cx);
                settings.availability("large-v2", true, cx);
                for page in [SettingsPage::System, SettingsPage::Appearance] {
                    settings.view.update(cx, |view, _| view.page = page);
                    window.render_frame(cx);
                    assert_eq!(settings.view.read(cx).transparency.entity_id(), slider_id);
                    assert_eq!(
                        settings.view.read(cx).transparency.read(cx).value().start(),
                        73.
                    );
                    assert!(
                        settings.take_save(cx).is_none(),
                        "editing is not a save/preview command"
                    );
                }
            }
            window.click("ok", cx);
            settings.take_save(cx).unwrap()
        })
        .unwrap();
    assert_eq!(saved.overlay_acrylic, Some(true));
    assert_eq!(saved.overlay_transparency, Some(73));
    let saved = Settings::from_json(&serde_json::to_string(&saved).unwrap()).unwrap();
    cx.update(|cx| {
        let reopened = SettingsWindow::open(saved, 42, "log", cx).unwrap();
        assert!(reopened.view.read(cx).draft.overlay_appearance().acrylic);
        assert_eq!(
            reopened.view.read(cx).transparency.read(cx).value().start(),
            73.
        );
        reopened.close(cx);
        settings.close(cx);
    });
}

#[gpui_kit::test]
fn prompt_height_survives_save_and_reopen(cx: &mut TestAppContext) {
    let settings = open(cx);
    let saved = cx
        .update_window(settings.handle, |_, window, cx| {
            settings
                .view
                .update(cx, |view, _| view.prompt_height = px(287.));
            settings.view.read(cx).read(window, cx).unwrap()
        })
        .unwrap();
    assert_eq!(saved.initial_prompt_height, Some(287.));
    let saved = Settings::from_json(&serde_json::to_string(&saved).unwrap()).unwrap();
    cx.update(|cx| {
        let reopened = SettingsWindow::open(saved, 42, "log", cx).unwrap();
        assert_eq!(reopened.view.read(cx).prompt_height, px(287.));
        reopened.close(cx);
        settings.close(cx);
    });
    assert!(Settings::from_json(r#"{"initialPromptHeight":79}"#).is_err());
    assert!(Settings::from_json(r#"{"initialPromptHeight":601}"#).is_err());
}

#[gpui_kit::test]
fn resize_and_dpi_keep_draft_identity_scroll_and_footer(cx: &mut TestAppContext) {
    let settings = open(cx);
    let prompt_id = cx.update(|cx| settings.view.read(cx).prompt.entity_id());
    cx.update_window(settings.handle, |_, window, cx| {
        settings.view.update(cx, |view, cx| {
            view.page = SettingsPage::Transcription;
            view.prompt.update(cx, |prompt, cx| {
                prompt.set_value("Незбережений текст 🙂\r\nEnglish", window, cx)
            });
        });
    })
    .unwrap();
    for scale in [1., 1.25, 1.5, 1.75, 2.] {
        cx.simulate_window_scale_factor_change(settings.handle, scale);
        for width in [900., 480., 638.] {
            cx.simulate_window_resize(settings.handle, size(px(width), px(520.)));
            cx.update_window(settings.handle, |_, window, cx| {
                window.render_frame(cx);
                let viewport = window.find("settings-scroll").bounds();
                let save = window.find("ok").bounds();
                assert!(save.top() >= viewport.bottom());
                assert!(save.bottom() <= window.viewport_size().height);
                let footer = window.find("settings-footer").bounds();
                let cancel = window.find("cancel").bounds();
                let apply = window.find("apply").bounds();
                for distance in [
                    save.top() - footer.top() - px(1.), // footer border
                    footer.bottom() - save.bottom(),
                    cancel.left() - save.right(),
                    apply.left() - cancel.right(),
                    footer.right() - apply.right(),
                ] {
                    assert!((distance - px(12.)).abs() <= px(1.));
                }
                window.scroll(
                    "settings-scroll",
                    ScrollDelta::Pixels(point(px(0.), px(-400.))),
                    cx,
                );
                let view = settings.view.read(cx);
                assert_eq!(
                    window
                        .find(("input", view.prompt.entity_id()))
                        .bounds()
                        .size
                        .height,
                    view.prompt_height,
                    "Prompt height is owned by the form, not available viewport height"
                );
                assert_eq!(view.prompt.entity_id(), prompt_id);
                assert_eq!(
                    view.prompt.read(cx).value().as_ref(),
                    "Незбережений текст 🙂\r\nEnglish"
                );
                assert!(view.scroll().offset().y <= px(0.));
            })
            .unwrap();
        }
    }
}

#[gpui_kit::test]
fn invalid_save_and_async_failure_leave_editable_draft(cx: &mut TestAppContext) {
    let settings = open(cx);
    cx.update_window(settings.handle, |_, window, cx| {
        settings.view.update(cx, |view, cx| {
            view.color
                .update(cx, |input, cx| input.set_value("invalid color", window, cx));
        });
        window.render_frame(cx);
        window.click("ok", cx);
        assert!(settings.take_save(cx).is_none());
        assert!(!settings.view.read(cx).busy);
        assert!(!settings.view.read(cx).error.is_empty());
        settings.view.update(cx, |view, cx| {
            view.color
                .update(cx, |input, cx| input.set_value("#00FF00", window, cx));
            view.hotkey.update(cx, |select, cx| {
                select.set_selected_index(Some(IndexPath::new(4)), window, cx)
            });
        });
        window.render_frame(cx);
        window.click("ok", cx);
        assert_eq!(settings.take_save(cx).unwrap().hotkey, "F8");
        settings.saved(&Err("Disk is read-only".into()), cx);
        window.render_frame(cx);
        assert!(!settings.is_closed());
        assert!(!settings.view.read(cx).busy);
        assert_eq!(
            settings
                .view
                .read(cx)
                .hotkey
                .read(cx)
                .selected_value()
                .unwrap(),
            "F8"
        );
        window.click("ok", cx);
        assert_eq!(settings.take_save(cx).unwrap().hotkey, "F8");
    })
    .unwrap();
}

#[gpui_kit::test]
fn cancel_does_not_submit_unsaved_edits(cx: &mut TestAppContext) {
    let settings = open(cx);
    cx.update_window(settings.handle, |_, window, cx| {
        settings.view.update(cx, |view, cx| {
            view.hotkey.update(cx, |select, cx| {
                select.set_selected_index(Some(IndexPath::new(5)), window, cx)
            });
            view.draft.start_at_login = true;
            view.draft.overlay_acrylic = Some(false);
            view.transparency
                .update(cx, |slider, cx| slider.set_value(90., window, cx));
        });
        window.render_frame(cx);
        window.click("cancel", cx);
        assert!(settings.is_closed());
        assert!(settings.take_save(cx).is_none());
    })
    .unwrap();
}

#[gpui_kit::test]
fn hotkey_dropdown_submits_all_six_choices_and_status_preserves_startup_draft(
    cx: &mut TestAppContext,
) {
    let settings = open(cx);
    cx.update_window(settings.handle, |_, window, cx| {
        for (index, (label, stored)) in HOTKEY_CHOICES.iter().enumerate() {
            settings.view.update(cx, |view, cx| {
                view.hotkey.update(cx, |select, cx| {
                    select.set_selected_index(Some(IndexPath::new(index)), window, cx)
                });
                view.draft.start_at_login = true;
            });
            settings.memory("Host: 80 MiB; Worker: 0 MiB", cx);
            settings.availability("large-v2", true, cx);
            assert_eq!(
                settings
                    .view
                    .read(cx)
                    .hotkey
                    .read(cx)
                    .selected_value()
                    .unwrap(),
                label
            );
            window.render_frame(cx);
            window.click("ok", cx);
            let request = settings.take_save(cx).unwrap();
            assert_eq!(&request.hotkey, stored);
            assert!(request.start_at_login);
            settings.saved(&Err("Simulated save error".into()), cx);
        }
    })
    .unwrap();
}

#[gpui_kit::test]
fn master_detail_keeps_drafts_prompt_size_and_save_all_pages(cx: &mut TestAppContext) {
    let settings = open(cx);
    cx.update_window(settings.handle, |_, window, cx| {
        let prompt_id = settings.view.read(cx).prompt.entity_id();
        settings.view.update(cx, |view, cx| {
            view.prompt
                .update(cx, |input, cx| input.set_value("Мій prompt 🙂", window, cx));
            view.prompt_height = px(280.);
            view.draft.start_at_login = true;
            view.draft.debug_logging = true;
            view.draft.keep_in_memory = false;
        });
        for index in [1usize, 2, 3, 0, 1] {
            window.render_frame(cx);
            window.click(("settings-page", index), cx);
            window.render_frame(cx);
            assert_eq!(settings.view.read(cx).page, SettingsPage::ALL[index]);
            assert_eq!(settings.view.read(cx).prompt.entity_id(), prompt_id);
            let viewport = window.find("settings-scroll").bounds();
            let navigation = window.find("settings-navigation").bounds();
            let detail = window.find("settings-detail").bounds();
            assert!(navigation.right() <= detail.left());
            assert!(detail.size.width > px(0.));
            if index == SettingsPage::System as usize {
                assert!(window.find("memory-report").bounds().size.height > px(0.));
            }
            assert!(window.find("ok").bounds().top() >= viewport.bottom());
        }
        window.click("ok", cx);
        let saved = settings.take_save(cx).unwrap();
        assert_eq!(saved.custom_initial_prompt, "Мій prompt 🙂");
        assert_eq!(saved.initial_prompt_height, Some(280.));
        assert!(saved.start_at_login && saved.debug_logging);
        assert!(!saved.keep_in_memory);
    })
    .unwrap();
}

#[gpui_kit::test]
fn model_status_is_below_model_dropdown_inside_transcription_not_footer(cx: &mut TestAppContext) {
    let settings = open(cx);
    cx.update_window(settings.handle, |_, window, cx| {
        let selected = settings.view.read(cx).status_model.clone();
        settings.availability(&selected, true, cx);
        settings
            .view
            .update(cx, |view, _| view.page = SettingsPage::Transcription);
        window.render_frame(cx);
        let dropdown = window.find("model-dropdown").bounds();
        let status = window.find("model-status").bounds();
        let scroll = window.find("settings-scroll").bounds();
        let footer = window.find("settings-footer").bounds();
        assert!(status.top() >= dropdown.bottom());
        assert!(status.bottom() <= scroll.bottom());
        assert!(status.bottom() <= footer.top());
        assert!(window.find("ok").bounds().top() >= footer.top());
        settings.availability(&selected, false, cx);
        assert_eq!(
            settings.view.read(cx).status,
            "The model needs to be downloaded"
        );
        assert!(!settings.view.read(cx).busy);
    })
    .unwrap();
}

#[gpui_kit::test]
fn model_selection_queries_draft_and_ignores_other_models(cx: &mut TestAppContext) {
    let settings = open(cx);
    cx.update_window(settings.handle, |_, window, cx| {
        let original = settings.model_query(cx).unwrap();
        settings.model_queried(cx);
        settings.availability(&original, true, cx);
        assert_eq!(settings.view.read(cx).status, "The model is ready");
        settings.view.update(cx, |view, cx| {
            view.model.update(cx, |select, cx| {
                select.set_selected_index(Some(IndexPath::new(1)), window, cx);
                cx.notify();
            });
        });
    })
    .unwrap();
    cx.run_until_parked();
    cx.update(|cx| {
        let selected = settings
            .model_query(cx)
            .expect("selection queues a read-only check");
        let original = Settings::initial().unwrap().model;
        assert_ne!(selected, original);
        assert_eq!(settings.view.read(cx).status_model, selected);
        assert_eq!(
            model_size_label(&settings.view.read(cx).status_model),
            model_size_label(&selected)
        );
        assert_eq!(settings.view.read(cx).status, "Checking model…");
        settings.availability(&original, true, cx);
        assert_eq!(settings.view.read(cx).status, "Checking model…");
        settings.availability(&selected, false, cx);
        assert_eq!(
            settings.view.read(cx).status,
            "The model needs to be downloaded"
        );
        assert!(
            settings.take_save(cx).is_none(),
            "selection does not save/download"
        );
    });
}
