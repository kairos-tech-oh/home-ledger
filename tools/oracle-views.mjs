// The prototype's figures for the Goals, Planning, Spending and Dashboard
// screens, printed in the same shape as `cargo run -p ledger-math --example
// views`, so the two can be diffed line for line.
//
//   node tools/oracle-views.mjs <prototype-checkout> <ledger.json> <today yyyy-mm-dd>
//
// Dates are taken at local midnight on both sides, which is where the two
// agree by design (see "Planning counts in calendar days" in docs/PORT.md).
import { readFileSync } from "node:fs";
import { createContext, runInContext } from "node:vm";

const [proto, ledgerPath, today] = process.argv.slice(2);
const load = (file) => {
  const ctx = createContext({});
  runInContext(readFileSync(`${proto}/core/${file}`, "utf8").replace(/^\.pragma library\s*/, ""), ctx);
  return ctx;
};
const M = load("Model.js");
const S = load("Spending.js");
const d = JSON.parse(readFileSync(ledgerPath, "utf8"));
const f = (n) => (Math.round(n * 100) / 100).toFixed(2);
const p1 = (n) => (n === null ? "-" : (Math.round(n * 10) / 10).toFixed(1));
const short = (id) => String(id).slice(0, 8);

const nowMs = Date.parse(`${today}T00:00:00`);
const adj = M.rothAdjustments(d.accounts || []);

console.log("goals");
for (const g of d.goals || []) {
  const saved = M.goalSaved(g, d.buckets, d.investments, adj);
  console.log(`  ${short(g.id)} saved ${f(saved)} progress ${p1(M.goalProgress(g, d.buckets, d.investments, adj))}`);
}

console.log("planning");
for (const months of [3, 12, 24]) {
  const to = new Date(nowMs);
  to.setMonth(to.getMonth() + months);
  const plan = M.project(d.buckets, d.budget, d.investments, adj, to.getTime(), nowMs);
  console.log(`  ${months}m months ${p1(plan.months)}`);
  for (const b of plan.buckets) {
    if (b.current === 0 && b.contributions === 0 && b.deductions === 0) continue;
    console.log(
      `    ${short(b.id)} now ${f(b.current)} in ${f(b.contributions)} out ${f(b.deductions)} ends ${f(b.projected)} pct ${p1(b.percent)} draws ${b.draws.length}`,
    );
  }
}

console.log("spending");
for (const period of ["1m", "3m", "1y", "all"]) {
  for (const status of ["all", "settled", "open"]) {
    const r = S.analyse(d.reconciliations, d.buckets, S.range(period, "", "", nowMs), status);
    console.log(
      `  ${period} ${status} total ${f(r.total)} count ${r.count} withdrawn ${f(r.withdrawn)} buckets ${f(r.fromBuckets)} everyday ${f(r.everyday)} unattributed ${f(r.unattributed)} unitemized ${f(r.unitemized)} average ${f(r.average)} undated ${r.undated} inferred ${r.inferredDates} people ${r.people.length} items ${r.items.length} months ${r.months.length}`,
    );
  }
}

console.log("paydays");
for (const share of M.ownerShares(d.income)) {
  const deltas = M.contributionDeltas(share.owner, d.buckets, d.budget, M.ownerShares(d.income), 1);
  if (!deltas.length) continue;
  console.log(`  ${share.owner} total ${f(deltas.reduce((t, x) => t + x.delta, 0))} buckets ${deltas.length}`);
  for (const x of deltas) console.log(`    ${short(x.id)} ${f(x.delta)}`);
}

console.log("dashboard");
console.log(`  default ${M.defaultDashboard(d).widgets.map((w) => w.kind).join(",")}`);
const c = M.creditRollup(d.accounts);
console.log(`  credit available ${f(c.available)} owed ${f(c.owed)} limit ${f(c.limit)} count ${c.count} recorded ${c.recorded}`);
const rec = M.widgetData(d, { kind: "reconciliation" }, { points: [] }, nowMs + 12 * 3600 * 1000);
console.log(`  reconciling open ${rec.open} balance ${f(rec.openBalance)} year ${f(rec.yearSpending)} buckets ${f(rec.yearBuckets)} days ${rec.daysSince}`);
const ret = M.widgetData(d, { kind: "retirement" }, { points: [] }, nowMs);
console.log(`  retirement roth ${f(ret.roth)} traditional ${f(ret.traditional)} contributions ${f(ret.rothContributions)}`);
const h = M.investmentRollup(d.investments);
console.log(`  holdings value ${f(h.value)} basis ${f(h.basis)} gain ${f(h.gain)} count ${h.count}`);
console.log(`  suggested buckets ${M.newWidget("buckets", d).refs.map(short).join(",")}`);
console.log(`  suggested accounts ${M.newWidget("accounts", d).refs.map(short).join(",")}`);
