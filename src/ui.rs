use eframe::egui;
use crate::config::{Config, SoundPreset, FlowSensitivity};
use crate::Event;
use std::sync::mpsc;

pub struct ConfigWindow {
    config: Config,
    tx: mpsc::Sender<Event>,
    message: Option<String>,
}

impl ConfigWindow {
    pub fn new(config: Config, tx: mpsc::Sender<Event>) -> Self {
        Self {
            config,
            tx,
            message: None,
        }
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("EyeFlow 设置");
            ui.separator();
            ui.add_space(8.0);

            ui.checkbox(&mut self.config.enabled, "启用护眼提醒");
            ui.add_space(4.0);

            egui::Grid::new("settings_grid")
                .striped(true)
                .num_columns(2)
                .spacing([16.0, 8.0])
                .show(ui, |ui| {
                    // 提醒间隔（UI 显示为分钟，内部存储为秒）
                    let mut min_min = self.config.min_interval_secs as u32 / 60;
                    ui.label("最小间隔（分钟）：");
                    ui.add(egui::Slider::new(&mut min_min, 5..=60));
                    self.config.min_interval_secs = (min_min as u64) * 60;
                    ui.end_row();

                    let mut max_min = self.config.max_interval_secs as u32 / 60;
                    ui.label("最大间隔（分钟）：");
                    ui.add(egui::Slider::new(&mut max_min, 5..=120));
                    self.config.max_interval_secs = (max_min as u64) * 60;
                    ui.end_row();

                    ui.label("护眼时长：");
                    ui.horizontal(|ui| {
                        ui.add(egui::Slider::new(&mut self.config.eye_rest_secs, 10..=60));
                        let secs = self.config.eye_rest_secs;
                        if secs < 60 {
                            ui.label(format!("{} 秒", secs));
                        } else {
                            ui.label(format!("{} 分 {} 秒", secs / 60, secs % 60));
                        }
                    });
                    ui.end_row();
                    ui.end_row();

                    ui.separator();
                    ui.separator();
                    ui.end_row();

                    // 提示音
                    ui.label("提示音：");
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut self.config.sound_enabled, "启用");
                    });
                    ui.end_row();

                    ui.label("提示音预设：");
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt("sound_preset")
                            .selected_text(match self.config.sound_preset {
                                SoundPreset::GentleChime => "风铃",
                                SoundPreset::SoftTap => "轻敲",
                                SoundPreset::WaterDrop => "水滴",
                                SoundPreset::DigitalDrop => "数字降调",
                                SoundPreset::TripleBeep => "三连短哔",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.config.sound_preset, SoundPreset::GentleChime, "风铃");
                                ui.selectable_value(&mut self.config.sound_preset, SoundPreset::SoftTap, "轻敲");
                                ui.selectable_value(&mut self.config.sound_preset, SoundPreset::WaterDrop, "水滴");
                                ui.selectable_value(&mut self.config.sound_preset, SoundPreset::DigitalDrop, "数字降调");
                                ui.selectable_value(&mut self.config.sound_preset, SoundPreset::TripleBeep, "三连短哔");
                            });
                        if ui.button("▶ 试听").clicked() {
                            crate::audio::preview_sound(&self.config.sound_preset);
                        }
                    });
                    ui.end_row();

                    ui.label("系统通知：");
                    ui.checkbox(&mut self.config.notification_enabled, "启用弹窗通知");
                    ui.end_row();

                    ui.separator();
                    ui.separator();
                    ui.end_row();

                    // 免打扰
                    ui.label("免打扰时段：");
                    ui.end_row();

                    ui.label("开始时间：");
                    ui.text_edit_singleline(&mut self.config.dnd_start);
                    ui.end_row();

                    ui.label("结束时间：");
                    ui.text_edit_singleline(&mut self.config.dnd_end);
                    ui.end_row();

                    ui.separator();
                    ui.separator();
                    ui.end_row();

                    // 心流检测
                    ui.label("心流检测灵敏度：");
                    ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                        egui::ComboBox::from_id_salt("sensitivity")
                            .selected_text(match self.config.flow_sensitivity {
                                FlowSensitivity::Low => "低（30秒内按键 > 6 次）",
                                FlowSensitivity::Medium => "中（30秒内按键 > 10 次）",
                                FlowSensitivity::High => "高（30秒内按键 > 15 次）",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.config.flow_sensitivity, FlowSensitivity::Low, "低（30秒内按键 > 6 次）");
                                ui.selectable_value(&mut self.config.flow_sensitivity, FlowSensitivity::Medium, "中（30秒内按键 > 10 次）");
                                ui.selectable_value(&mut self.config.flow_sensitivity, FlowSensitivity::High, "高（30秒内按键 > 15 次）");
                            });
                        ui.label(egui::RichText::new(
                            "灵敏度越高，键盘敲得越快才会被判定为「心流状态」。\n心流状态下提醒会延后，等键盘停歇后再触发。",
                        ).size(12.0).color(egui::Color32::GRAY));
                    });
                    ui.end_row();

                    ui.label("全局快捷键：");
                    ui.text_edit_singleline(&mut self.config.global_mute_hotkey);
                    ui.end_row();
                });

            ui.add_space(16.0);

            if ui.button("保存设置").clicked() {
                let new_config = self.config.clone();
                let _ = self.tx.send(Event::SettingsChanged(new_config));
                self.message = Some("设置已保存".to_string());
            }

            if let Some(msg) = &self.message {
                ui.label(egui::RichText::new(msg).color(egui::Color32::GREEN));
            }
        });
    }

    #[allow(dead_code)]
    pub fn config(&self) -> &Config {
        &self.config
    }
}
