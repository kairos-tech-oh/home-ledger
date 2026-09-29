// Load Model.js the way the web server does: strip the QML-only pragma line.
import { readFileSync } from "node:fs";
const src = readFileSync(process.argv[2], "utf8");
const js = src.startsWith(".pragma") ? "//" + src : src;
const mod = await import("data:text/javascript," + encodeURIComponent(js + "\nexport { sheetRollup, incomeRollup, budgetRollup, ownerShares, retirementRollup, projectBalance };"));
const d = JSON.parse(readFileSync(process.argv[3], "utf8"));

const sheet = mod.sheetRollup(d.accounts, d.investments);
const income = mod.incomeRollup(d.income);
const budget = mod.budgetRollup(d.budget);
const f = (n) => (Math.round(n * 100) / 100).toFixed(2);
console.log("assets        ", f(sheet.assets));
console.log("debts         ", f(sheet.debts));
console.log("net           ", f(sheet.net));
console.log("monthly income", f(income.monthly ?? income.total ?? 0));
console.log("monthly budget", f(budget.monthly ?? budget.total ?? 0));
const ret = mod.retirementRollup(d.accounts, d.investments, d.budget, d.buckets, Date.now());
console.log("retirement    ", f(ret.total), "monthly", f(ret.monthly), `over ${ret.count} accounts`);
for (const rate of [6, 8, 10]) {
  const pts = mod.projectBalance(ret.total, ret.monthly, rate, 30);
  const last = pts[pts.length - 1];
  console.log(`project 30y @${rate}%`, f(last.value), "contributed", f(last.contributed));
}
for (const o of mod.ownerShares(d.income)) {
  console.log(
    `earner ${o.owner}`,
    f(o.monthly),
    `${(Math.round(o.percent * 100) / 100).toFixed(2)}%`,
    `${(Math.round(o.paychecksPerMonth * 10000) / 10000).toFixed(4)}/mo`,
  );
}
