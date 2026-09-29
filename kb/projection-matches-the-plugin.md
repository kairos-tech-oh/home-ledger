---
id: home-ledger.projection-matches-the-plugin
project: home-ledger
category: logic
severity: warn
environment: any
depends_on: [home-ledger.core-tests-pass, home-ledger.a-projection-compounds-in-decimal]
---

# The retirement projection matches the plugin's to the dollar

## Claim
At the plugin's rates of 4, 6, 8 and 10 percent, `project_balance` lands within
five cents of `core/Model.js` `projectBalance` on fixed inputs, and the view
projects to the per-machine target year from settings, or shows nothing without one.

## Why
The chart replaces the plugin's, so a figure that differs from what the plugin
showed on the same ledger reads as money that appeared or vanished.

## Check
```bash
cargo test -p ledger-math the_projection_matches_the_plugin_to_the_dollar
cargo test -p ledger-math every_month_is_on_the_same_curve_as_the_yearly_points
cargo test -p home-ledger the_projection_uses_the_plugins_rates_and_calendar
cargo test -p home-ledger no_target_year_projects_nothing
cargo test -p home-ledger a_target_year_is_kept_only_within_seventy_years_from_now
cargo test -p home-ledger the_target_year_is_seeded_from_the_plugin_once
grep -q 'Set a target retirement year to project forward' ui/src/Projection.svelte
```

## Depends On
[[core-tests-pass]]
[[a-projection-compounds-in-decimal]]
