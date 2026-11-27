import { AnimatePresence, motion } from "motion/react";
import { useEffect, useId, useRef, useState, type FormEvent, type KeyboardEvent } from "react";
import { errorMessage } from "../../api/client";
import { useAddQuestion, useDeleteQuestion, useResetChecklist, useUpdateQuestion } from "../../api/queries";
import type { ChecklistKind, ChecklistQuestion, Portfolio } from "../../api/types";
import { IconEdit, IconPlus, IconTrash } from "../../components/icons";
import { Grid, Page, PageHeader, Panel } from "../../components/Layout";
import { MagneticButton } from "../../components/MagneticButton";
import { useToast } from "../../components/toastContext";
import { CHECKLIST_CLASSES, CHECKLIST_LABEL, CLASS_META } from "../../lib/assetClasses";
import { plural } from "../../lib/format";

type CriteriaScreenProps = { portfolio: Portfolio };

const KINDS: ChecklistKind[] = ["stocks", "real_estate"];

export function CriteriaScreen({ portfolio }: Readonly<CriteriaScreenProps>) {
  return (
    <Page className="criteria">
      <PageHeader
        title="As perguntas que dão a nota"
        infoLabel="Como a nota é calculada"
        info={
          <>
            Cada <strong className="yes">sim</strong> soma 1, cada <strong className="no">não</strong> tira 1, e o que ficou sem
            resposta não conta. A nota nunca fica negativa — e só ativo com nota acima de zero recebe aporte.
          </>
        }
      />
      <Grid cols={2} alignStart>
        {KINDS.map((kind) => (
          <ChecklistCard
            key={kind}
            kind={kind}
            questions={portfolio.checklists[kind]}
            usage={portfolio.assets.filter((a) => CHECKLIST_CLASSES[kind].includes(a.asset_class)).length}
          />
        ))}
      </Grid>
    </Page>
  );
}

type ChecklistCardProps = { kind: ChecklistKind; questions: ChecklistQuestion[]; usage: number };

function ChecklistCard({ kind, questions, usage }: Readonly<ChecklistCardProps>) {
  const inputId = useId();
  const [draft, setDraft] = useState("");
  const [confirmReset, setConfirmReset] = useState(false);
  const add = useAddQuestion();
  const reset = useResetChecklist();
  const toast = useToast();
  const classes = CHECKLIST_CLASSES[kind].map((c) => CLASS_META[c].label).join(" e ");

  const submit = (event: FormEvent) => {
    event.preventDefault();
    const prompt = draft.trim();
    if (!prompt) return;
    add.mutate(
      { checklist: kind, prompt },
      {
        onSuccess: () => setDraft(""),
        onError: (error) => toast({ tone: "error", message: errorMessage(error) }),
      },
    );
  };

  const restore = () => {
    reset.mutate(kind, {
      onSuccess: () => {
        setConfirmReset(false);
        toast({ tone: "info", message: `Perguntas de ${CHECKLIST_LABEL[kind]} voltaram ao padrão.` });
      },
      onError: (error) => toast({ tone: "error", message: errorMessage(error) }),
    });
  };

  return (
    <Panel
      className="checklist"
      title={CHECKLIST_LABEL[kind]}
      actions={
        <span className="checklist__count">
          {questions.length}
          <span className="sr-only"> {plural(questions.length, ["pergunta", "perguntas"])}</span>
        </span>
      }
    >
      <p className="field__hint">
        Vale para {classes} · {usage} {plural(usage, ["ativo usa", "ativos usam"])}
      </p>

      <ol className="checklist__list">
        <AnimatePresence initial={false}>
          {questions.map((question, index) => (
            <motion.li
              key={question.id}
              layout
              initial={{ opacity: 0, height: 0 }}
              animate={{ opacity: 1, height: "auto" }}
              exit={{ opacity: 0, height: 0 }}
              className="checklist__item"
            >
              <QuestionRow question={question} number={index + 1} />
            </motion.li>
          ))}
        </AnimatePresence>
      </ol>
      {questions.length === 0 && <p className="field__hint">Sem perguntas: os ativos daqui ficam com nota 0.</p>}

      <form className="checklist__add" onSubmit={submit}>
        <label htmlFor={inputId} className="sr-only">
          Nova pergunta para {CHECKLIST_LABEL[kind]}
        </label>
        <input
          id={inputId}
          className="text-input"
          type="text"
          placeholder="Nova pergunta de sim ou não…"
          value={draft}
          onChange={(event) => setDraft(event.target.value)}
        />
        <MagneticButton type="submit" variant="soft" magnetic={false} disabled={!draft.trim()} loading={add.isPending} aria-label="Adicionar pergunta">
          <IconPlus />
        </MagneticButton>
      </form>

      <div className="checklist__reset">
        {confirmReset ? (
          <fieldset className="danger-zone__confirm" aria-label="Confirmar restauração">
            <span>Trocar pelas perguntas padrão? As respostas atuais somem.</span>
            <MagneticButton variant="danger" magnetic={false} loading={reset.isPending} onClick={restore}>
              Restaurar
            </MagneticButton>
            <MagneticButton variant="ghost" magnetic={false} onClick={() => setConfirmReset(false)}>
              Cancelar
            </MagneticButton>
          </fieldset>
        ) : (
          <button type="button" className="link-btn" onClick={() => setConfirmReset(true)}>
            Restaurar padrão
          </button>
        )}
      </div>
    </Panel>
  );
}

function QuestionRow({ question, number }: Readonly<{ question: ChecklistQuestion; number: number }>) {
  const inputId = useId();
  const [editing, setEditing] = useState(false);
  const [text, setText] = useState(question.prompt);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const update = useUpdateQuestion();
  const remove = useDeleteQuestion();
  const toast = useToast();
  const editRef = useRef<HTMLInputElement>(null);

  // Foco na própria edição: a pessoa acabou de pedir para editar esta linha.
  useEffect(() => {
    if (editing) editRef.current?.focus();
  }, [editing]);

  const save = () => {
    const prompt = text.trim();
    if (!prompt || prompt === question.prompt) {
      setEditing(false);
      setText(question.prompt);
      return;
    }
    update.mutate(
      { id: question.id, prompt },
      {
        onSuccess: () => setEditing(false),
        onError: (error) => toast({ tone: "error", message: errorMessage(error) }),
      },
    );
  };

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Enter") {
      event.preventDefault();
      save();
    } else if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      setEditing(false);
      setText(question.prompt);
    }
  };

  if (editing) {
    return (
      <div className="question question--editing">
        <span className="question__number">{number}</span>
        <label htmlFor={inputId} className="sr-only">
          Editar pergunta {number}
        </label>
        <input
          id={inputId}
          className="text-input"
          type="text"
          value={text}
          ref={editRef}
          onChange={(event) => setText(event.target.value)}
          onKeyDown={onKeyDown}
        />
        <MagneticButton variant="soft" magnetic={false} loading={update.isPending} onClick={save}>
          Salvar
        </MagneticButton>
      </div>
    );
  }

  return (
    <div className="question">
      <span className="question__number">{number}</span>
      <p className="question__prompt">{question.prompt}</p>
      {confirmDelete ? (
        <fieldset className="question__confirm" aria-label="Confirmar remoção">
          <MagneticButton variant="danger" magnetic={false} loading={remove.isPending} onClick={() => remove.mutate(question.id)}>
            Remover
          </MagneticButton>
          <MagneticButton variant="ghost" magnetic={false} onClick={() => setConfirmDelete(false)}>
            Manter
          </MagneticButton>
        </fieldset>
      ) : (
        <div className="question__actions">
          <button type="button" className="icon-btn" aria-label={`Editar pergunta ${number}`} onClick={() => setEditing(true)}>
            <IconEdit />
          </button>
          <button
            type="button"
            className="icon-btn icon-btn--danger"
            aria-label={`Remover pergunta ${number}`}
            onClick={() => setConfirmDelete(true)}
          >
            <IconTrash />
          </button>
        </div>
      )}
    </div>
  );
}
