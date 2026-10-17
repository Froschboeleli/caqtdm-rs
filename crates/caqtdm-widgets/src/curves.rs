//! Cartesian and strip sample buffers.
//!
//! `caCartesianPlot::displayData` decides whether a monitor is one point, a
//! waveform, or a ring of `count` scalars. `caStripPlot::RescaleCurves` and
//! `TimersStart` turn `period`, `units`, and `refreshRate` into the two timers.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurveKind {
    Both,
    XOnly,
    YOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlotMode {
    /// `plMode::PlotNPointsAndStop` (enum 0).
    Stop,
    /// `plMode::PlotLastNPoints`.
    Last,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CurveSlot {
    pub kind: CurveKind,
    pub x: Vec<f64>,
    pub y: Vec<f64>,
    pub accum_x: Vec<f64>,
    pub accum_y: Vec<f64>,
    pub shown_x: Vec<f64>,
    pub shown_y: Vec<f64>,
}

impl CurveSlot {
    pub fn new(kind: CurveKind) -> Self {
        Self {
            kind,
            x: Vec::new(),
            y: Vec::new(),
            accum_x: Vec::new(),
            accum_y: Vec::new(),
            shown_x: Vec::new(),
            shown_y: Vec::new(),
        }
    }

    /// `fillData` for `CH_X` / `CH_Y`. An absent side is left alone so a later
    /// monitor can fill it. `X_only` / `Y_only` fill the other side with indexes.
    pub fn store(&mut self, x: Option<&[f64]>, y: Option<&[f64]>) {
        if let Some(x) = x {
            self.x = x.to_vec();
        }
        if let Some(y) = y {
            self.y = y.to_vec();
        }
        if self.kind == CurveKind::XOnly && self.y.len() != self.x.len() {
            self.y = (0..self.x.len()).map(|i| i as f64).collect();
        } else if self.kind == CurveKind::YOnly && self.x.len() != self.y.len() {
            self.x = (0..self.y.len()).map(|i| i as f64).collect();
        }
    }

    /// `displayData`. `count` is `thisCountNumber` (0 until a count arrives).
    pub fn display(&mut self, mode: PlotMode, count: i32) {
        if self.x.is_empty() || self.y.is_empty() {
            return;
        }
        if self.x.len() > 1 && self.y.len() == 1 {
            let aux = self.y[0];
            self.y = vec![aux; self.x.len()];
            self.publish(limit(self.x.len(), count));
        } else if self.x.len() == 1 && self.y.len() > 1 {
            let aux = self.x[0];
            self.x = vec![aux; self.y.len()];
            self.publish(limit(self.y.len(), count));
        } else if self.x.len() == 1 && self.y.len() == 1 {
            if count <= 1 {
                self.publish(1);
            } else {
                self.accumulate(mode, count as usize);
            }
        } else {
            self.publish(limit(self.x.len().min(self.y.len()), count));
        }
    }

    pub fn erase(&mut self) {
        self.x.clear();
        self.y.clear();
        self.accum_x.clear();
        self.accum_y.clear();
        self.shown_x.clear();
        self.shown_y.clear();
    }

    fn publish(&mut self, n: usize) {
        let n = n.min(self.x.len()).min(self.y.len());
        self.shown_x = self.x[..n].to_vec();
        self.shown_y = self.y[..n].to_vec();
    }

    fn accumulate(&mut self, mode: PlotMode, count: usize) {
        if mode == PlotMode::Stop {
            if self.accum_x.len() >= count {
                return;
            }
        } else if self.accum_x.len() > count {
            self.accum_x.clear();
            self.accum_y.clear();
        } else if self.accum_x.len() >= count {
            match self.kind {
                CurveKind::XOnly => {
                    self.accum_x.remove(0);
                }
                CurveKind::YOnly => {
                    self.accum_y.remove(0);
                }
                CurveKind::Both => {
                    self.accum_x.remove(0);
                    self.accum_y.remove(0);
                }
            }
        }
        let x0 = self.x[0];
        let y0 = self.y[0];
        match self.kind {
            CurveKind::XOnly => {
                if self.accum_x.len() < count {
                    self.accum_y.push(self.accum_y.len() as f64);
                }
                self.accum_x.push(x0);
            }
            CurveKind::YOnly => {
                if self.accum_x.len() < count {
                    self.accum_x.push(self.accum_x.len() as f64);
                }
                self.accum_y.push(y0);
            }
            CurveKind::Both => {
                self.accum_x.push(x0);
                self.accum_y.push(y0);
            }
        }
        let n = self.accum_x.len().min(self.accum_y.len());
        self.shown_x = self.accum_x[..n].to_vec();
        self.shown_y = self.accum_y[..n].to_vec();
    }
}

fn limit(len: usize, count: i32) -> usize {
    if count > 0 {
        len.min(count as usize)
    } else {
        len
    }
}

/// Timers from `RescaleCurves` and `TimersStart`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StripTiming {
    /// Data-thread period in milliseconds, after the floor of 2.
    pub sample_ms: u64,
    /// Display timer: `max(timeInterval, 400|200|100)` for Low, Medium, High.
    pub paint_ms: u64,
    /// X-axis span in seconds (`INTERVAL`).
    pub interval_secs: f64,
}

pub fn strip_timing(period: f64, units: &str, history: i32, refresh: &str) -> StripTiming {
    let history = history.max(1) as f64;
    let (mut time_interval, mut interval_secs) = if units.eq_ignore_ascii_case("Millisecond") {
        (period / history, period / 1000.0)
    } else if units.eq_ignore_ascii_case("Minute") {
        (1000.0 * period * 60.0 / history, period * 60.0)
    } else {
        (1000.0 * period / history, period)
    };
    // Millisecond and Second both grow the span so the data timer is at least 2 ms.
    if !units.eq_ignore_ascii_case("Minute") && time_interval < 2.0 {
        time_interval = 2.0;
        interval_secs = time_interval * history / 1000.0;
    }
    let floor = if refresh.eq_ignore_ascii_case("High") {
        100
    } else if refresh.eq_ignore_ascii_case("Low") {
        400
    } else {
        200
    };
    StripTiming {
        sample_ms: (time_interval as u64).max(1),
        paint_ms: (time_interval as u64).max(floor),
        interval_secs,
    }
}

/// `channels_N` split on `;`, the same three branches as `scanWidgets`.
pub fn parse_curve(text: &str) -> Option<(CurveKind, String, String)> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    if !text.contains(';') {
        return Some((CurveKind::YOnly, String::new(), text.to_string()));
    }
    let mut parts = text.splitn(2, ';');
    let x = parts.next().unwrap_or("").trim();
    let y = parts.next().unwrap_or("").trim();
    if x.is_empty() && y.is_empty() {
        None
    } else if y.is_empty() {
        Some((CurveKind::XOnly, x.to_string(), String::new()))
    } else if x.is_empty() {
        Some((CurveKind::YOnly, String::new(), y.to_string()))
    } else {
        Some((CurveKind::Both, x.to_string(), y.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn y_only(values: &[f64], mode: PlotMode, count: i32) -> Vec<f64> {
        let mut slot = CurveSlot::new(CurveKind::YOnly);
        for value in values {
            slot.store(None, Some(&[*value]));
            slot.display(mode, count);
        }
        slot.shown_y
    }

    #[test]
    fn stop_ignores_the_point_past_count_and_last_shifts() {
        assert_eq!(y_only(&[1.0, 2.0, 9.0], PlotMode::Stop, 2), vec![1.0, 2.0]);
        assert_eq!(y_only(&[1.0, 2.0, 9.0], PlotMode::Last, 2), vec![2.0, 9.0]);
        assert_eq!(y_only(&[4.0, 5.0], PlotMode::Stop, 0), vec![5.0]);
    }

    #[test]
    fn waveforms_repeat_a_scalar_and_honor_count() {
        let mut slot = CurveSlot::new(CurveKind::Both);
        slot.store(Some(&[0.0, 9.0, 10.0]), Some(&[0.0, 8.0, 0.0]));
        slot.display(PlotMode::Stop, 0);
        assert_eq!(slot.shown_x, vec![0.0, 9.0, 10.0]);
        assert_eq!(slot.shown_y, vec![0.0, 8.0, 0.0]);

        slot.store(Some(&[0.0, 1.0, 2.0, 3.0]), Some(&[7.0]));
        slot.display(PlotMode::Stop, 3);
        assert_eq!(slot.shown_y, vec![7.0, 7.0, 7.0]);

        slot.erase();
        assert!(slot.shown_y.is_empty());
    }

    #[test]
    fn strip_timer_floors_the_interval_and_clamps_refresh() {
        let slow = strip_timing(1.0, "Second", 72, "Low");
        assert_eq!(slow.sample_ms, 13);
        assert_eq!(slow.paint_ms, 400);
        let floored = strip_timing(1.0, "Second", 1_000, "Medium");
        assert_eq!(floored.sample_ms, 2);
        assert_eq!(floored.interval_secs, 2.0);
        let fast = strip_timing(1.0, "Second", 72, "High");
        assert_eq!(fast.paint_ms, 100);
        let minute = strip_timing(1.0, "Minute", 10, "Medium");
        assert_eq!(minute.sample_ms, 6_000);
        assert_eq!(minute.interval_secs, 60.0);
    }
}
