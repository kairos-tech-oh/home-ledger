// The pill colour for a history entry's action. No imports, so the kb can run it.

export type Tone = "blue" | "green" | "amber" | "red" | "violet" | "muted";

const tones: Record<string, Tone> = {
  create: "blue",
  add: "green",
  update: "amber",
  edit: "amber",
  remove: "red",
  delete: "red",
  move: "violet",
};

/** Imported history can say anything; what is not known stays muted. */
export function actionTone(action: string): Tone {
  return tones[action.trim().toLowerCase()] ?? "muted";
}
