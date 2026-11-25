import type { ChecklistQuestion } from "../api/types";

export type Answer = boolean | null;

/**
 * A mesma regra do backend (docs/api.md § Nota): cada sim soma 1, cada não
 * tira 1, pergunta sem resposta não conta, e a nota nunca fica negativa.
 * Só entram respostas de perguntas que ainda existem.
 */
export function checklistScore(answers: Record<string, Answer>, questions: ChecklistQuestion[]): number {
  let total = 0;
  for (const q of questions) {
    const answer = answers[q.id];
    if (answer === true) total += 1;
    else if (answer === false) total -= 1;
  }
  return Math.max(0, total);
}

export function answeredCount(answers: Record<string, Answer>, questions: ChecklistQuestion[]): number {
  return questions.filter((q) => typeof answers[q.id] === "boolean").length;
}

/** O que mudou entre o salvo e o rascunho, no formato do PATCH (`null` apaga). */
export function answersDiff(
  saved: Record<string, boolean>,
  draft: Record<string, Answer>,
): Record<string, Answer> | null {
  const diff: Record<string, Answer> = {};
  const ids = new Set([...Object.keys(saved), ...Object.keys(draft)]);
  for (const id of ids) {
    const before = id in saved ? saved[id] : null;
    const after = id in draft ? draft[id] : null;
    if ((before ?? null) !== (after ?? null)) diff[id] = after ?? null;
  }
  return Object.keys(diff).length ? diff : null;
}
