//! # agent-audience
//!
//! Audience feedback as system validation.
//!
//! The audience IS the test suite. If they leave, your system failed.
//! This crate models audience members, aggregate responses, approval tracking,
//! feedback signals, audience segmentation, and post-performance surveys —
//! treating every observer as a source of ground truth about system quality.

use std::collections::HashMap;

// ── Audience member ────────────────────────────────────────────────────────

/// An individual feedback provider in the audience.
#[derive(Debug, Clone)]
pub struct AudienceMember {
    pub id: String,
    pub segment: String,
    /// How engaged this member is: 0.0 (asleep) to 1.0 (leaning forward).
    pub engagement: f64,
    /// Historical satisfaction: 0.0 to 1.0.
    pub satisfaction: f64,
    /// Number of performances this member has attended.
    pub performances_attended: usize,
}

impl AudienceMember {
    pub fn new(id: &str, segment: &str) -> Self {
        Self {
            id: id.to_string(),
            segment: segment.to_string(),
            engagement: 0.5,
            satisfaction: 0.5,
            performances_attended: 0,
        }
    }

    pub fn with_engagement(mut self, e: f64) -> Self {
        self.engagement = e.clamp(0.0, 1.0);
        self
    }

    pub fn with_satisfaction(mut self, s: f64) -> Self {
        self.satisfaction = s.clamp(0.0, 1.0);
        self
    }

    /// Record that this member attended another performance.
    pub fn attend(&mut self, satisfaction: f64) {
        let n = self.performances_attended as f64;
        // Running average update.
        self.satisfaction = (self.satisfaction * n + satisfaction.clamp(0.0, 1.0)) / (n + 1.0);
        self.performances_attended += 1;
    }

    /// Would this member recommend the performance?
    pub fn would_recommend(&self) -> bool {
        self.satisfaction >= 0.7
    }
}

// ── Audience response ──────────────────────────────────────────────────────

/// A single response from an audience member.
#[derive(Debug, Clone)]
pub struct Response {
    pub member_id: String,
    pub rating: f64, // 0.0 – 1.0
    pub timestamp: u64,
}

/// Aggregate response across the audience.
#[derive(Debug, Clone)]
pub struct AudienceResponse {
    pub responses: Vec<Response>,
}

impl AudienceResponse {
    pub fn new() -> Self {
        Self {
            responses: Vec::new(),
        }
    }

    pub fn add(&mut self, response: Response) {
        self.responses.push(response);
    }

    pub fn count(&self) -> usize {
        self.responses.len()
    }

    /// Average rating across all responses.
    pub fn average_rating(&self) -> f64 {
        if self.responses.is_empty() {
            return 0.0;
        }
        self.responses.iter().map(|r| r.rating).sum::<f64>() / self.responses.len() as f64
    }

    /// Median rating.
    pub fn median_rating(&self) -> f64 {
        if self.responses.is_empty() {
            return 0.0;
        }
        let mut ratings: Vec<f64> = self.responses.iter().map(|r| r.rating).collect();
        ratings.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mid = ratings.len() / 2;
        if ratings.len() % 2 == 0 {
            (ratings[mid - 1] + ratings[mid]) / 2.0
        } else {
            ratings[mid]
        }
    }

    /// Standard deviation of ratings.
    pub fn rating_stddev(&self) -> f64 {
        if self.responses.len() < 2 {
            return 0.0;
        }
        let mean = self.average_rating();
        let variance = self
            .responses
            .iter()
            .map(|r| (r.rating - mean).powi(2))
            .sum::<f64>()
            / (self.responses.len() - 1) as f64;
        variance.sqrt()
    }

    /// Net Promoter–style score: fraction rating ≥ 0.7 minus fraction < 0.4.
    pub fn net_score(&self) -> f64 {
        if self.responses.is_empty() {
            return 0.0;
        }
        let promoters = self.responses.iter().filter(|r| r.rating >= 0.7).count();
        let detractors = self.responses.iter().filter(|r| r.rating < 0.4).count();
        (promoters as f64 - detractors as f64) / self.responses.len() as f64
    }

    /// Split responses by a segment key (e.g. "vip", "general").
    pub fn by_segment(&self, members: &[AudienceMember]) -> HashMap<String, AudienceResponse> {
        let member_segments: HashMap<&str, &str> = members
            .iter()
            .map(|m| (m.id.as_str(), m.segment.as_str()))
            .collect();

        let mut map: HashMap<String, AudienceResponse> = HashMap::new();
        for resp in &self.responses {
            let seg = member_segments
                .get(resp.member_id.as_str())
                .copied()
                .unwrap_or("unknown");
            map.entry(seg.to_string())
                .or_insert_with(AudienceResponse::new)
                .add(resp.clone());
        }
        map
    }
}

impl Default for AudienceResponse {
    fn default() -> Self {
        Self::new()
    }
}

// ── Approval rating ────────────────────────────────────────────────────────

/// Track audience satisfaction over time.
#[derive(Debug, Clone)]
pub struct ApprovalRating {
    pub history: Vec<TimestampedRating>,
    pub window_size: usize,
}

#[derive(Debug, Clone)]
pub struct TimestampedRating {
    pub timestamp: u64,
    pub rating: f64,
}

impl ApprovalRating {
    pub fn new(window_size: usize) -> Self {
        Self {
            history: Vec::new(),
            window_size,
        }
    }

    pub fn record(&mut self, timestamp: u64, rating: f64) {
        self.history.push(TimestampedRating {
            timestamp,
            rating: rating.clamp(0.0, 1.0),
        });
        // Trim to window.
        if self.history.len() > self.window_size {
            let excess = self.history.len() - self.window_size;
            self.history.drain(..excess);
        }
    }

    /// Current rolling average approval.
    pub fn current(&self) -> f64 {
        if self.history.is_empty() {
            return 0.0;
        }
        self.history.iter().map(|t| t.rating).sum::<f64>() / self.history.len() as f64
    }

    /// Is approval trending up, down, or flat?
    pub fn trend(&self) -> ApprovalTrend {
        if self.history.len() < 3 {
            return ApprovalTrend::Flat;
        }
        let mid = self.history.len() / 2;
        let first_half: f64 = self.history[..mid].iter().map(|t| t.rating).sum::<f64>() / mid as f64;
        let second_half: f64 = self.history[mid..].iter().map(|t| t.rating).sum::<f64>()
            / (self.history.len() - mid) as f64;

        let diff = second_half - first_half;
        if diff > 0.05 {
            ApprovalTrend::Improving
        } else if diff < -0.05 {
            ApprovalTrend::Declining
        } else {
            ApprovalTrend::Flat
        }
    }

    /// Min and max ratings in the window.
    pub fn range(&self) -> (f64, f64) {
        if self.history.is_empty() {
            return (0.0, 0.0);
        }
        let min = self.history.iter().map(|t| t.rating).fold(f64::INFINITY, f64::min);
        let max = self.history.iter().map(|t| t.rating).fold(f64::NEG_INFINITY, f64::max);
        (min, max)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalTrend {
    Improving,
    Declining,
    Flat,
}

// ── Feedback signal ────────────────────────────────────────────────────────

/// Specific feedback about what worked or didn't.
#[derive(Debug, Clone)]
pub struct FeedbackSignal {
    pub member_id: String,
    pub category: FeedbackCategory,
    pub sentiment: f64,  // -1.0 (terrible) to 1.0 (amazing)
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FeedbackCategory {
    Performance,
    Reliability,
    Latency,
    Usability,
    Communication,
    Overall,
}

impl FeedbackSignal {
    pub fn is_positive(&self) -> bool {
        self.sentiment > 0.2
    }

    pub fn is_negative(&self) -> bool {
        self.sentiment < -0.2
    }

    pub fn is_neutral(&self) -> bool {
        self.sentiment.abs() <= 0.2
    }
}

/// Aggregate feedback signals by category.
#[derive(Debug, Clone)]
pub struct FeedbackSummary {
    pub by_category: HashMap<FeedbackCategory, CategorySummary>,
}

#[derive(Debug, Clone)]
pub struct CategorySummary {
    pub count: usize,
    pub avg_sentiment: f64,
    pub positive_count: usize,
    pub negative_count: usize,
}

impl FeedbackSummary {
    pub fn from_signals(signals: &[FeedbackSignal]) -> Self {
        let mut by_category: HashMap<FeedbackCategory, Vec<&FeedbackSignal>> = HashMap::new();
        for sig in signals {
            by_category.entry(sig.category).or_default().push(sig);
        }

        let by_category: HashMap<FeedbackCategory, CategorySummary> = by_category
            .into_iter()
            .map(|(cat, sigs)| {
                let count = sigs.len();
                let avg_sentiment = sigs.iter().map(|s| s.sentiment).sum::<f64>() / count as f64;
                let positive_count = sigs.iter().filter(|s| s.is_positive()).count();
                let negative_count = sigs.iter().filter(|s| s.is_negative()).count();
                (
                    cat,
                    CategorySummary {
                        count,
                        avg_sentiment,
                        positive_count,
                        negative_count,
                    },
                )
            })
            .collect();

        Self { by_category }
    }

    /// The worst-performing category.
    pub fn worst_category(&self) -> Option<(FeedbackCategory, &CategorySummary)> {
        self.by_category
            .iter()
            .min_by(|a, b| a.1.avg_sentiment.partial_cmp(&b.1.avg_sentiment).unwrap())
            .map(|(k, v)| (*k, v))
    }

    /// The best-performing category.
    pub fn best_category(&self) -> Option<(FeedbackCategory, &CategorySummary)> {
        self.by_category
            .iter()
            .max_by(|a, b| a.1.avg_sentiment.partial_cmp(&b.1.avg_sentiment).unwrap())
            .map(|(k, v)| (*k, v))
    }
}

// ── Audience segment ───────────────────────────────────────────────────────

/// Different audience types want different things.
#[derive(Debug, Clone)]
pub struct AudienceSegment {
    pub name: String,
    pub members: Vec<AudienceMember>,
    pub expectations: HashMap<FeedbackCategory, f64>,
}

impl AudienceSegment {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            members: Vec::new(),
            expectations: HashMap::new(),
        }
    }

    pub fn with_expectation(mut self, category: FeedbackCategory, min_score: f64) -> Self {
        self.expectations.insert(category, min_score);
        self
    }

    pub fn add_member(&mut self, member: AudienceMember) {
        self.members.push(member);
    }

    pub fn size(&self) -> usize {
        self.members.len()
    }

    /// Average satisfaction of members in this segment.
    pub fn avg_satisfaction(&self) -> f64 {
        if self.members.is_empty() {
            return 0.0;
        }
        self.members.iter().map(|m| m.satisfaction).sum::<f64>() / self.members.len() as f64
    }

    /// Check if a category meets the segment's expectations.
    pub fn meets_expectation(&self, category: FeedbackCategory, actual: f64) -> bool {
        self.expectations
            .get(&category)
            .map(|min| actual >= *min)
            .unwrap_or(true)
    }
}

// ── Concert survey ─────────────────────────────────────────────────────────

/// Post-performance evaluation.
#[derive(Debug, Clone)]
pub struct ConcertSurvey {
    pub performance_id: String,
    pub responses: AudienceResponse,
    pub signals: Vec<FeedbackSignal>,
    pub timestamp: u64,
}

impl ConcertSurvey {
    pub fn new(performance_id: &str, timestamp: u64) -> Self {
        Self {
            performance_id: performance_id.to_string(),
            responses: AudienceResponse::new(),
            signals: Vec::new(),
            timestamp,
        }
    }

    pub fn add_response(&mut self, response: Response) {
        self.responses.add(response);
    }

    pub fn add_signal(&mut self, signal: FeedbackSignal) {
        self.signals.push(signal);
    }

    /// Overall pass/fail based on a threshold.
    pub fn passes(&self, threshold: f64) -> bool {
        self.responses.average_rating() >= threshold
    }

    /// Generate a summary report.
    pub fn report(&self) -> SurveyReport {
        let feedback_summary = FeedbackSummary::from_signals(&self.signals);
        SurveyReport {
            performance_id: self.performance_id.clone(),
            avg_rating: self.responses.average_rating(),
            median_rating: self.responses.median_rating(),
            response_count: self.responses.count(),
            net_score: self.responses.net_score(),
            feedback_summary,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SurveyReport {
    pub performance_id: String,
    pub avg_rating: f64,
    pub median_rating: f64,
    pub response_count: usize,
    pub net_score: f64,
    pub feedback_summary: FeedbackSummary,
}

impl SurveyReport {
    /// Human-readable verdict.
    pub fn verdict(&self) -> &str {
        if self.avg_rating >= 0.9 {
            "Standing Ovation"
        } else if self.avg_rating >= 0.7 {
            "Warm Applause"
        } else if self.avg_rating >= 0.5 {
            "Polite Clapping"
        } else if self.avg_rating >= 0.3 {
            "Scattered Booing"
        } else {
            "Walking Out"
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ── AudienceMember tests ───────────────────────────────────────────

    #[test]
    fn member_creation() {
        let m = AudienceMember::new("m1", "vip").with_engagement(0.8).with_satisfaction(0.9);
        assert_eq!(m.id, "m1");
        assert_eq!(m.segment, "vip");
        assert!((m.engagement - 0.8).abs() < 1e-9);
        assert!(m.would_recommend());
    }

    #[test]
    fn member_attend_updates_satisfaction() {
        let mut m = AudienceMember::new("m1", "general");
        assert!((m.satisfaction - 0.5).abs() < 1e-9);
        m.attend(0.9);
        assert!((m.satisfaction - 0.9).abs() < 1e-9);
        m.attend(0.3);
        // (0.9 + 0.3) / 2 = 0.6
        assert!((m.satisfaction - 0.6).abs() < 1e-9);
        assert_eq!(m.performances_attended, 2);
    }

    #[test]
    fn member_would_not_recommend() {
        let m = AudienceMember::new("m1", "general").with_satisfaction(0.3);
        assert!(!m.would_recommend());
    }

    #[test]
    fn member_engagement_clamped() {
        let m = AudienceMember::new("m1", "general").with_engagement(2.0);
        assert!(m.engagement <= 1.0);
        let m2 = AudienceMember::new("m2", "general").with_engagement(-1.0);
        assert!(m2.engagement >= 0.0);
    }

    // ── AudienceResponse tests ─────────────────────────────────────────

    #[test]
    fn response_empty() {
        let r = AudienceResponse::new();
        assert_eq!(r.count(), 0);
        assert_eq!(r.average_rating(), 0.0);
        assert_eq!(r.median_rating(), 0.0);
    }

    #[test]
    fn response_averages() {
        let mut r = AudienceResponse::new();
        r.add(Response { member_id: "a".into(), rating: 0.8, timestamp: 1 });
        r.add(Response { member_id: "b".into(), rating: 0.6, timestamp: 2 });
        assert!((r.average_rating() - 0.7).abs() < 1e-9);
        assert!((r.median_rating() - 0.7).abs() < 1e-9);
    }

    #[test]
    fn response_median_odd() {
        let mut r = AudienceResponse::new();
        r.add(Response { member_id: "a".into(), rating: 0.2, timestamp: 1 });
        r.add(Response { member_id: "b".into(), rating: 0.8, timestamp: 2 });
        r.add(Response { member_id: "c".into(), rating: 0.5, timestamp: 3 });
        assert!((r.median_rating() - 0.5).abs() < 1e-9);
    }

    #[test]
    fn response_stddev() {
        let mut r = AudienceResponse::new();
        r.add(Response { member_id: "a".into(), rating: 1.0, timestamp: 1 });
        r.add(Response { member_id: "b".into(), rating: 0.0, timestamp: 2 });
        assert!((r.rating_stddev() - 0.7071).abs() < 0.01);
    }

    #[test]
    fn response_net_score() {
        let mut r = AudienceResponse::new();
        r.add(Response { member_id: "a".into(), rating: 0.9, timestamp: 1 }); // promoter
        r.add(Response { member_id: "b".into(), rating: 0.8, timestamp: 2 }); // promoter
        r.add(Response { member_id: "c".into(), rating: 0.3, timestamp: 3 }); // detractor
        // (2 - 1) / 3 ≈ 0.333
        assert!((r.net_score() - 0.333).abs() < 0.01);
    }

    #[test]
    fn response_by_segment() {
        let members = vec![
            AudienceMember::new("a", "vip"),
            AudienceMember::new("b", "vip"),
            AudienceMember::new("c", "general"),
        ];
        let mut r = AudienceResponse::new();
        r.add(Response { member_id: "a".into(), rating: 0.9, timestamp: 1 });
        r.add(Response { member_id: "b".into(), rating: 0.8, timestamp: 2 });
        r.add(Response { member_id: "c".into(), rating: 0.5, timestamp: 3 });

        let by_seg = r.by_segment(&members);
        assert_eq!(by_seg["vip"].count(), 2);
        assert_eq!(by_seg["general"].count(), 1);
    }

    // ── ApprovalRating tests ───────────────────────────────────────────

    #[test]
    fn approval_empty() {
        let ar = ApprovalRating::new(10);
        assert_eq!(ar.current(), 0.0);
        assert_eq!(ar.trend(), ApprovalTrend::Flat);
        assert_eq!(ar.range(), (0.0, 0.0));
    }

    #[test]
    fn approval_rolling_window() {
        let mut ar = ApprovalRating::new(3);
        ar.record(1, 0.5);
        ar.record(2, 0.6);
        ar.record(3, 0.7);
        ar.record(4, 0.8); // pushes out 0.5
        assert!((ar.current() - 0.7).abs() < 1e-9); // avg of 0.6, 0.7, 0.8
    }

    #[test]
    fn approval_trend_improving() {
        let mut ar = ApprovalRating::new(100);
        for i in 0..10 {
            ar.record(i, 0.3 + i as f64 * 0.07);
        }
        assert_eq!(ar.trend(), ApprovalTrend::Improving);
    }

    #[test]
    fn approval_trend_declining() {
        let mut ar = ApprovalRating::new(100);
        for i in 0..10 {
            ar.record(i, 0.9 - i as f64 * 0.07);
        }
        assert_eq!(ar.trend(), ApprovalTrend::Declining);
    }

    #[test]
    fn approval_range() {
        let mut ar = ApprovalRating::new(100);
        ar.record(1, 0.3);
        ar.record(2, 0.9);
        ar.record(3, 0.5);
        assert_eq!(ar.range(), (0.3, 0.9));
    }

    // ── FeedbackSignal tests ───────────────────────────────────────────

    #[test]
    fn signal_positive() {
        let s = FeedbackSignal {
            member_id: "m1".into(),
            category: FeedbackCategory::Performance,
            sentiment: 0.8,
            message: "Loved it".into(),
        };
        assert!(s.is_positive());
        assert!(!s.is_negative());
    }

    #[test]
    fn signal_negative() {
        let s = FeedbackSignal {
            member_id: "m1".into(),
            category: FeedbackCategory::Reliability,
            sentiment: -0.7,
            message: "Crashed".into(),
        };
        assert!(s.is_negative());
        assert!(!s.is_positive());
    }

    #[test]
    fn signal_neutral() {
        let s = FeedbackSignal {
            member_id: "m1".into(),
            category: FeedbackCategory::Overall,
            sentiment: 0.0,
            message: "Meh".into(),
        };
        assert!(s.is_neutral());
    }

    // ── FeedbackSummary tests ──────────────────────────────────────────

    #[test]
    fn summary_from_signals() {
        let signals = vec![
            FeedbackSignal {
                member_id: "a".into(),
                category: FeedbackCategory::Performance,
                sentiment: 0.8,
                message: "Great".into(),
            },
            FeedbackSignal {
                member_id: "b".into(),
                category: FeedbackCategory::Performance,
                sentiment: -0.5,
                message: "Slow".into(),
            },
            FeedbackSignal {
                member_id: "c".into(),
                category: FeedbackCategory::Reliability,
                sentiment: 0.9,
                message: "Solid".into(),
            },
        ];
        let summary = FeedbackSummary::from_signals(&signals);
        assert_eq!(summary.by_category.len(), 2);
        let perf = &summary.by_category[&FeedbackCategory::Performance];
        assert_eq!(perf.count, 2);
        assert!((perf.avg_sentiment - 0.15).abs() < 1e-9);
        assert_eq!(perf.positive_count, 1);
        assert_eq!(perf.negative_count, 1);
    }

    #[test]
    fn summary_best_worst() {
        let signals = vec![
            FeedbackSignal {
                member_id: "a".into(),
                category: FeedbackCategory::Performance,
                sentiment: 0.9,
                message: "Great".into(),
            },
            FeedbackSignal {
                member_id: "b".into(),
                category: FeedbackCategory::Latency,
                sentiment: -0.8,
                message: "Slow".into(),
            },
        ];
        let summary = FeedbackSummary::from_signals(&signals);
        assert_eq!(summary.best_category().unwrap().0, FeedbackCategory::Performance);
        assert_eq!(summary.worst_category().unwrap().0, FeedbackCategory::Latency);
    }

    // ── AudienceSegment tests ──────────────────────────────────────────

    #[test]
    fn segment_creation() {
        let seg = AudienceSegment::new("vip")
            .with_expectation(FeedbackCategory::Performance, 0.8)
            .with_expectation(FeedbackCategory::Reliability, 0.9);
        assert_eq!(seg.name, "vip");
        assert_eq!(seg.size(), 0);
        assert_eq!(seg.expectations.len(), 2);
    }

    #[test]
    fn segment_satisfaction() {
        let mut seg = AudienceSegment::new("general");
        seg.add_member(AudienceMember::new("a", "general").with_satisfaction(0.6));
        seg.add_member(AudienceMember::new("b", "general").with_satisfaction(0.8));
        assert!((seg.avg_satisfaction() - 0.7).abs() < 1e-9);
    }

    #[test]
    fn segment_meets_expectations() {
        let seg = AudienceSegment::new("vip")
            .with_expectation(FeedbackCategory::Reliability, 0.9);
        assert!(seg.meets_expectation(FeedbackCategory::Reliability, 0.95));
        assert!(!seg.meets_expectation(FeedbackCategory::Reliability, 0.8));
        // No expectation for Performance → always meets.
        assert!(seg.meets_expectation(FeedbackCategory::Performance, 0.1));
    }

    // ── ConcertSurvey tests ────────────────────────────────────────────

    #[test]
    fn survey_pass_fail() {
        let mut s = ConcertSurvey::new("perf-1", 100);
        s.add_response(Response { member_id: "a".into(), rating: 0.8, timestamp: 100 });
        s.add_response(Response { member_id: "b".into(), rating: 0.9, timestamp: 101 });
        assert!(s.passes(0.7));
        assert!(!s.passes(0.95));
    }

    #[test]
    fn survey_report_verdict() {
        let mut s = ConcertSurvey::new("perf-2", 200);
        s.add_response(Response { member_id: "a".into(), rating: 0.95, timestamp: 200 });
        assert_eq!(s.report().verdict(), "Standing Ovation");

        let mut s2 = ConcertSurvey::new("perf-3", 300);
        s2.add_response(Response { member_id: "a".into(), rating: 0.2, timestamp: 300 });
        assert_eq!(s2.report().verdict(), "Walking Out");
    }

    #[test]
    fn survey_with_signals() {
        let mut s = ConcertSurvey::new("perf-4", 400);
        s.add_response(Response { member_id: "a".into(), rating: 0.7, timestamp: 400 });
        s.add_signal(FeedbackSignal {
            member_id: "a".into(),
            category: FeedbackCategory::Usability,
            sentiment: 0.5,
            message: "Could use better docs".into(),
        });
        let report = s.report();
        assert_eq!(report.response_count, 1);
        assert!(!report.feedback_summary.by_category.is_empty());
    }

    #[test]
    fn survey_full_verdict_range() {
        let cases = vec![
            (0.95, "Standing Ovation"),
            (0.75, "Warm Applause"),
            (0.55, "Polite Clapping"),
            (0.35, "Scattered Booing"),
            (0.15, "Walking Out"),
        ];
        for (rating, expected) in cases {
            let mut s = ConcertSurvey::new("test", 0);
            s.add_response(Response { member_id: "a".into(), rating, timestamp: 0 });
            assert_eq!(s.report().verdict(), expected, "Failed for rating {rating}");
        }
    }
}
