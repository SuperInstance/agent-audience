# agent-audience

**Audience feedback as system validation.**

> The audience IS the test suite. If they leave, your system failed.

This crate models audience members, aggregate responses, approval tracking,
feedback signals, audience segmentation, and post-performance surveys —
treating every observer as a source of ground truth about system quality.

## Core Concepts

### Audience Member

An `AudienceMember` is an individual feedback provider. Each member has:

- An **ID** and **segment** (e.g. "vip", "general", "internal")
- **Engagement** (0.0–1.0) — how actively they're paying attention
- **Satisfaction** (0.0–1.0) — rolling average of historical performance ratings
- **Attendance count** — how many performances they've been through

Members update their satisfaction via `attend()`, which maintains a running
average. A member `would_recommend()` when satisfaction ≥ 0.7.

### Audience Response

`AudienceResponse` aggregates individual ratings into meaningful statistics:

- **Average rating** — the mean across all responses
- **Median rating** — the middle value, resistant to outliers
- **Standard deviation** — how much opinions diverge
- **Net score** — a Net Promoter-style metric: fraction of promoters (≥ 0.7)
  minus fraction of detractors (< 0.4), giving a single number from -1.0 to 1.0
- **Segment breakdown** — split responses by audience segment for targeted analysis

### Approval Rating

`ApprovalRating` tracks satisfaction over time with a rolling window. Beyond
the current score, it detects **trends** — Improving, Declining, or Flat — by
comparing the first and second halves of the window. A shift of ±0.05 triggers
a trend change.

This is the health metric for your system over time. A single bad performance
is a blip; a declining trend is a crisis.

### Feedback Signal

`FeedbackSignal` captures specific, actionable feedback about what worked and
what didn't. Each signal has:

- A **category** (Performance, Reliability, Latency, Usability, Communication, Overall)
- A **sentiment** (-1.0 to 1.0, where negative = bad, positive = good)
- A **message** (free text explaining the sentiment)

Signals can be classified as positive (> 0.2), negative (< -0.2), or neutral.

`FeedbackSummary` aggregates signals by category, showing which aspects of the
system are strongest and weakest. It identifies the best and worst categories,
making it easy to prioritise improvements.

### Audience Segment

`AudienceSegment` models different audience types who want different things.
A VIP segment might expect ≥ 0.9 reliability, while a general audience is
happy with ≥ 0.7. Segments have:

- Named members
- Per-category **expectations** (minimum acceptable scores)
- Aggregate satisfaction tracking

Use `meets_expectation()` to check if a category's actual performance satisfies
a particular segment's requirements.

### Concert Survey

`ConcertSurvey` is a post-performance evaluation that bundles responses and
feedback signals into a single assessable unit. Key operations:

- `passes(threshold)` — did the performance meet a minimum bar?
- `report()` — generate a `SurveyReport` with full statistics and feedback summary
- `verdict()` — a human-readable label:

| Average Rating | Verdict          |
|----------------|------------------|
| ≥ 0.90         | Standing Ovation |
| ≥ 0.70         | Warm Applause    |
| ≥ 0.50         | Polite Clapping  |
| ≥ 0.30         | Scattered Booing |
| < 0.30         | Walking Out      |

## Usage

```rust
use agent_audience::*;

// Build an audience.
let mut vip = AudienceSegment::new("vip")
    .with_expectation(FeedbackCategory::Reliability, 0.9)
    .with_expectation(FeedbackCategory::Performance, 0.8);
vip.add_member(AudienceMember::new("alice", "vip").with_satisfaction(0.85));
vip.add_member(AudienceMember::new("bob", "vip").with_satisfaction(0.90));

// Run a survey after a performance.
let mut survey = ConcertSurvey::new("deploy-2024-001", 1700000000);
survey.add_response(Response { member_id: "alice".into(), rating: 0.92, timestamp: 1700000001 });
survey.add_response(Response { member_id: "bob".into(), rating: 0.87, timestamp: 1700000002 });
survey.add_signal(FeedbackSignal {
    member_id: "alice".into(),
    category: FeedbackCategory::Latency,
    sentiment: -0.4,
    message: "Slightly slower than usual".into(),
});

// Evaluate.
let report = survey.report();
println!("Average: {:.2} — {}", report.avg_rating, report.verdict());
println!("Net score: {:.2}", report.net_score);
if let Some((cat, summary)) = report.feedback_summary.worst_category() {
    println!("Weakest area: {:?} (sentiment: {:.2})", cat, summary.avg_sentiment);
}
```

## Why Audience?

Every system has an audience — users, SREs, other services, automated monitors.
Their feedback is the most honest metric you have. Unlike synthetic benchmarks
or self-reported health checks, the audience tells you:

- **Whether the system actually works** (not whether it *should* work)
- **What matters most** (segment expectations reveal priorities)
- **Whether things are getting better or worse** (approval trends)
- **Specifically what to fix** (feedback signals by category)

The musical metaphor extends naturally: a concert succeeds when the audience
applauds. A deployment succeeds when the audience's approval rating holds or
improves. Standing ovation = ship it. Walking out = rollback.

## License

MIT
