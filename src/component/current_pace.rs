//! Provides the Current Pace Component and relevant types for using it. The
//! Current Pace Component is a component that shows a prediction of the current
//! attempt's final time, if the current attempt's pace matches the chosen
//! comparison for the remainder of the run.

use super::key_value;
use crate::{
    analysis::current_pace,
    comparison,
    platform::prelude::*,
    platform::to_local,
    settings::{Color, Field, Gradient, SettingsDescription, Value},
    timing::{
        formatter::{Accuracy, Regular, TimeFormatter, DASH},
        Snapshot,
    },
    TimerPhase,
};
use alloc::borrow::Cow;
use core::fmt::Write;
use serde_derive::{Deserialize, Serialize};

use time::{
    macros::format_description,
    format_description::BorrowedFormatItem,
};

/// The Current Pace Component is a component that shows a prediction of the
/// current attempt's final time, if the current attempt's pace matches the
/// chosen comparison for the remainder of the run.
#[derive(Default, Clone)]
pub struct Component {
    settings: Settings,
}

/// The Settings for this component.
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// The background shown behind the component.
    pub background: Gradient,
    /// The comparison chosen. Uses the Timer's current comparison if set to
    /// `None`.
    pub comparison_override: Option<String>,
    /// Specifies whether to display the name of the component and its value in
    /// two separate rows.
    pub display_two_rows: bool,
    /// The color of the label. If `None` is specified, the color is taken from
    /// the layout.
    pub label_color: Option<Color>,
    /// The color of the value. If `None` is specified, the color is taken from
    /// the layout.
    pub value_color: Option<Color>,
    /// The accuracy of the time shown.
    pub accuracy: Accuracy,
    /// Display predicted time relative wall clock
    pub wall_clock: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            background: key_value::DEFAULT_GRADIENT,
            comparison_override: None,
            display_two_rows: false,
            label_color: None,
            value_color: None,
            accuracy: Accuracy::Seconds,
            wall_clock: false,
        }
    }
}

const DEFAULT_WALL_CLOCK_FORMAT: &[BorrowedFormatItem<'_>] = format_description!("[hour]:[minute]:[second]");

impl Component {
    /// Creates a new Current Pace Component.
    pub fn new() -> Self {
        Default::default()
    }

    /// Creates a new Current Pace Component with the given settings.
    pub const fn with_settings(settings: Settings) -> Self {
        Self { settings }
    }

    /// Accesses the settings of the component.
    pub const fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Grants mutable access to the settings of the component.
    pub fn settings_mut(&mut self) -> &mut Settings {
        &mut self.settings
    }

    /// Accesses the name of the component.
    pub fn name(&self) -> Cow<'static, str> {
        self.text(self.settings.comparison_override.as_deref())
    }

    fn text(&self, comparison: Option<&str>) -> Cow<'static, str> {
        if let Some(comparison) = comparison {
            match comparison {
                comparison::personal_best::NAME => "Current Pace".into(),
                comparison::best_segments::NAME => "Best Possible Time".into(),
                comparison::worst_segments::NAME => "Worst Possible Time".into(),
                comparison::average_segments::NAME => "Predicted Time".into(),
                comparison => format!("Current Pace ({})", comparison::shorten(comparison)).into(),
            }
        } else {
            "Current Pace".into()
        }
    }

    /// Updates the component's state based on the timer provided.
    pub fn update_state(&self, state: &mut key_value::State, timer: &Snapshot<'_>) {
        let comparison = comparison::resolve(&self.settings.comparison_override, timer);
        let comparison = comparison::or_current(comparison, timer);
        let key = self.text(Some(comparison));

        state.background = self.settings.background;
        state.key_color = self.settings.label_color;
        state.value_color = self.settings.value_color;
        state.semantic_color = Default::default();

        state.key.clear();
        state.key.push_str(&key); // FIXME: Uncow this

        state.value.clear();

        if !self.settings.wall_clock {
            let (current_pace, uf) =
                if timer.current_phase() == TimerPhase::NotRunning && key.starts_with("Current Pace") {
                    (None, false)
                } else {
                    current_pace::calculate(timer, comparison)
                };

            state.updates_frequently = uf;

            let _ = write!(
                state.value,
                "{}",
                Regular::with_accuracy(self.settings.accuracy).format(current_pace)
            );
        } else {
            let (predicted_time, uf) = current_pace::predict_wall_clock_time(timer, comparison);

            state.updates_frequently = uf;

            if let Some(pt) = predicted_time {
                let value = to_local(pt.time).format(DEFAULT_WALL_CLOCK_FORMAT).unwrap();
                let _ = write!(state.value, "{}", value);
            } else {
                let _ = write!(state.value, "{}", DASH);
            }
        }

        state.key_abbreviations.clear();
        // FIXME: This &* probably is different when key is uncowed
        match &*key {
            "Best Possible Time" => {
                state.key_abbreviations.push("Best Poss. Time".into());
                state.key_abbreviations.push("Best Time".into());
                state.key_abbreviations.push("BPT".into());
            }
            "Worst Possible Time" => {
                state.key_abbreviations.push("Worst Poss. Time".into());
                state.key_abbreviations.push("Worst Time".into());
            }
            "Predicted Time" => {
                state.key_abbreviations.push("Pred. Time".into());
            }
            "Current Pace" => {
                state.key_abbreviations.push("Cur. Pace".into());
                state.key_abbreviations.push("Pace".into());
            }
            _ => {
                state.key_abbreviations.push("Current Pace".into());
                state.key_abbreviations.push("Cur. Pace".into());
                state.key_abbreviations.push("Pace".into());
            }
        }

        state.display_two_rows = self.settings.display_two_rows;
    }

    /// Calculates the component's state based on the timer provided.
    pub fn state(&self, timer: &Snapshot<'_>) -> key_value::State {
        let mut state = Default::default();
        self.update_state(&mut state, timer);
        state
    }

    /// Accesses a generic description of the settings available for this
    /// component and their current values.
    pub fn settings_description(&self) -> SettingsDescription {
        SettingsDescription::with_fields(vec![
            Field::new(
                "Background".into(),
                "The background shown behind the component.".into(),
                self.settings.background.into(),
            ),
            Field::new(
                "Comparison".into(),
                "The comparison to predict the final time from. If not specified, the current comparison is used.".into(),
                self.settings.comparison_override.clone().into(),
            ),
            Field::new(
                "Display 2 Rows".into(),
                "Specifies whether to display the name of the component and the predicted time in two separate rows.".into(),
                self.settings.display_two_rows.into(),
            ),
            Field::new(
                "Label Color".into(),
                "The color of the component's name. If not specified, the color is taken from the layout.".into(),
                self.settings.label_color.into(),
            ),
            Field::new(
                "Value Color".into(),
                "The color of the predicted time. If not specified, the color is taken from the layout.".into(),
                self.settings.value_color.into(),
            ),
            Field::new(
                "Accuracy".into(),
                "The accuracy of the predicted time shown.".into(),
                self.settings.accuracy.into(),
            ),
            Field::new(
                "Display relative wall clock".into(),
                "Display the predicted wall clock time".into(),
                self.settings.wall_clock.into(),
            ),
        ])
    }

    /// Sets a setting's value by its index to the given value.
    ///
    /// # Panics
    ///
    /// This panics if the type of the value to be set is not compatible with
    /// the type of the setting's value. A panic can also occur if the index of
    /// the setting provided is out of bounds.
    pub fn set_value(&mut self, index: usize, value: Value) {
        match index {
            0 => self.settings.background = value.into(),
            1 => self.settings.comparison_override = value.into(),
            2 => self.settings.display_two_rows = value.into(),
            3 => self.settings.label_color = value.into(),
            4 => self.settings.value_color = value.into(),
            5 => self.settings.accuracy = value.into(),
            6 => self.settings.wall_clock = value.into(),
            _ => panic!("Unsupported Setting Index"),
        }
    }
}
