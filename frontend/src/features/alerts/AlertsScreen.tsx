import { useEffect, useId, useMemo, useState, type ReactNode } from "react";
import { errorMessage } from "../../api/client";
import type { Alerts, AlertKind, AlertsRequest, AlertType, LinkCode } from "../../api/alertsTypes";
import { GlassCard } from "../../components/GlassCard";
import { InfoTip } from "../../components/InfoTip";
import { Grid, Page, PageHeader, Panel } from "../../components/Layout";
import { MagneticButton } from "../../components/MagneticButton";
import { Skeleton } from "../../components/Skeleton";
import { useToast } from "../../components/toastContext";
import { KIND_TEXT, kindName, sampleDigest } from "../../lib/alertsText";
import { formatDay, parseDecimal } from "../../lib/format";
import { useMoney } from "../../lib/privacy";
import { useAlerts, useLinkTelegram, useSaveAlerts, useSendTest, useUnlinkTelegram } from "./useAlerts";

const HOUR_PATTERN = /^([01]\d|2[0-3]):[0-5]\d$/;

/** De quanto em quanto tempo a tela confere se o vínculo fechou. */
const PAIRING_POLL_MS = 3000;

export function AlertsScreen() {
  const [pairing, setPairing] = useState<LinkCode | null>(null);
  const alerts = useAlerts(pairing !== null ? PAIRING_POLL_MS : false);
  const toast = useToast();
  const linked = alerts.data?.telegram.linked ?? false;

  // A pessoa mandou o código ao bot e o servidor já ligou o chat.
  useEffect(() => {
    if (pairing && linked) {
      setPairing(null);
      toast({ tone: "success", message: "Telegram vinculado." });
    }
  }, [pairing, linked, toast]);

  return (
    <Page className="alerts">
      <PageHeader
        title="Alertas no Telegram"
        infoLabel="Sobre os alertas"
        info="Um resumo por dia, só quando há algo a dizer: nunca uma mensagem por evento. Você escolhe o que avisar, a hora e se o resumo leva valores."
      />

      {alerts.isPending && (
        <GlassCard as="section" aria-label="Carregando os alertas" className="alerts__loading">
          <Skeleton height={28} width="55%" />
          <Skeleton height={64} />
        </GlassCard>
      )}
      {alerts.isError && (
        <GlassCard as="section" role="alert" className="alerts__error">
          <p>{errorMessage(alerts.error)}</p>
          <MagneticButton variant="ghost" magnetic={false} onClick={() => void alerts.refetch()}>
            Tentar de novo
          </MagneticButton>
        </GlassCard>
      )}
      {alerts.data &&
        (alerts.data.telegram.configured ? (
          <PreferencesCard
            alerts={alerts.data}
            telegram={<TelegramCard alerts={alerts.data} pairing={pairing} onPairing={setPairing} />}
          />
        ) : (
          <TelegramCard alerts={alerts.data} pairing={pairing} onPairing={setPairing} />
        ))}
    </Page>
  );
}

function TelegramCard({
  alerts,
  pairing,
  onPairing,
}: {
  alerts: Alerts;
  pairing: LinkCode | null;
  onPairing: (code: LinkCode | null) => void;
}) {
  const { telegram } = alerts;
  const toast = useToast();
  const link = useLinkTelegram();
  const unlink = useUnlinkTelegram();
  const test = useSendTest();

  if (!telegram.configured) {
    return (
      <Panel className="alerts__card" title="Telegram">
        <p role="status">
          Os alertas estão desligados neste servidor: falta o token do bot do Telegram (a variável{" "}
          <code>TELEGRAM_BOT_TOKEN</code>). Nada é enviado enquanto ele não existir.
        </p>
        <p className="field__hint">
          O passo a passo, do bot no @BotFather ao segredo no servidor, está em <code>docs/alertas.md</code>.
        </p>
      </Panel>
    );
  }

  const start = () =>
    link.mutate(undefined, {
      onSuccess: (code) => onPairing(code),
      onError: (error) => toast({ tone: "error", message: errorMessage(error) }),
    });
  const stop = () =>
    unlink.mutate(undefined, {
      onSuccess: () => toast({ tone: "success", message: "Telegram desvinculado." }),
      onError: (error) => toast({ tone: "error", message: errorMessage(error) }),
    });
  const sendTest = () =>
    test.mutate(undefined, {
      onSuccess: () => toast({ tone: "success", message: "Teste enviado. Confira o Telegram." }),
      onError: (error) => toast({ tone: "error", message: errorMessage(error) }),
    });

  return (
    <Panel className="alerts__card" title="Telegram">
      {telegram.linked ? (
        <>
          <p role="status">
            Vinculado{telegram.linked_at ? ` em ${formatDay(telegram.linked_at)}` : ""}. O resumo chega no chat com o bot
            {telegram.bot_username ? ` @${telegram.bot_username}` : ""}.
          </p>
          <div className="alerts__actions">
            <MagneticButton variant="soft" loading={test.isPending} onClick={sendTest}>
              Enviar teste agora
            </MagneticButton>
            <MagneticButton variant="ghost" magnetic={false} loading={unlink.isPending} onClick={stop}>
              Desvincular
            </MagneticButton>
          </div>
        </>
      ) : pairing ? (
        <Pairing code={pairing} botUsername={pairing.bot_username ?? telegram.bot_username} onCancel={() => onPairing(null)} />
      ) : (
        <>
          <p>
            Nenhum chat vinculado. Gere um código e mande ao bot: o Prumo descobre o seu chat sozinho, você não precisa
            procurar número nenhum.
          </p>
          <div className="alerts__actions">
            <MagneticButton loading={link.isPending} onClick={start}>
              Vincular o Telegram
            </MagneticButton>
          </div>
        </>
      )}
    </Panel>
  );
}

function expiryTime(iso: string): string {
  const date = new Date(iso);
  return Number.isNaN(date.getTime()) ? "" : date.toLocaleTimeString("pt-BR", { hour: "2-digit", minute: "2-digit" });
}

function Pairing({ code, botUsername, onCancel }: { code: LinkCode; botUsername: string | null; onCancel: () => void }) {
  return (
    <div className="alerts__pairing">
      <ol className="alerts__steps">
        <li>
          Abra o bot no Telegram
          {botUsername ? (
            <>
              :{" "}
              <a className="link-btn" href={`https://t.me/${botUsername}?start=${code.code}`} target="_blank" rel="noopener noreferrer">
                @{botUsername}
              </a>
              . O link já leva o código.
            </>
          ) : (
            "."
          )}
        </li>
        <li>
          Se preferir digitar, mande esta mensagem: <code className="alerts__code">{code.command}</code>
        </li>
        <li>Esta tela confere sozinha e avisa quando o vínculo fechar.</li>
      </ol>
      <p className="field__hint">
        O código vale uma vez só{expiryTime(code.expires_at) ? `, até ${expiryTime(code.expires_at)}` : ""}.
      </p>
      <button type="button" className="link-btn" onClick={onCancel}>
        Cancelar
      </button>
    </div>
  );
}

type Draft = {
  send_at: string;
  hide_values: boolean;
  types: Record<AlertKind, { enabled: boolean; threshold: string }>;
};

const comma = (value: number | null) => (value === null ? "" : String(value).replace(".", ","));

function toDraft(alerts: Alerts): Draft {
  const types = Object.fromEntries(
    alerts.types.map((t) => [t.kind, { enabled: t.enabled, threshold: comma(t.threshold) }]),
  ) as Draft["types"];
  return { send_at: alerts.send_at, hide_values: alerts.hide_values, types };
}

function thresholdError(type: AlertType, text: string): string | null {
  if (type.unit === null) return null;
  const value = parseDecimal(text);
  if (value === null || value <= 0 || value > 100) return "Informe um número maior que 0 e até 100.";
  return null;
}

/**
 * O resto da tela, com o Telegram já configurado, em duas colunas: à esquerda
 * o vínculo e o que avisar; à direita quando e com quê, e o exemplo do resumo.
 */
function PreferencesCard({ alerts, telegram }: { alerts: Alerts; telegram: ReactNode }) {
  const saved = useMemo(() => toDraft(alerts), [alerts]);
  const [draft, setDraft] = useState<Draft>(saved);
  const [touched, setTouched] = useState(false);
  const save = useSaveAlerts();
  const toast = useToast();
  const money = useMoney();
  const baseId = useId();

  // Enquanto a pessoa não mexeu, a tela acompanha o que o servidor diz.
  useEffect(() => {
    if (!touched) setDraft(saved);
  }, [saved, touched]);

  const errors = useMemo(() => {
    const found: Partial<Record<AlertKind, string>> = {};
    for (const type of alerts.types) {
      if (!type.available || !draft.types[type.kind].enabled) continue;
      const message = thresholdError(type, draft.types[type.kind].threshold);
      if (message) found[type.kind] = message;
    }
    return found;
  }, [alerts.types, draft.types]);
  const hourError = HOUR_PATTERN.test(draft.send_at) ? null : "Informe a hora no formato HH:MM.";

  const changes = useMemo(() => {
    const body: AlertsRequest = {};
    if (draft.send_at !== saved.send_at && !hourError) body.send_at = draft.send_at;
    if (draft.hide_values !== saved.hide_values) body.hide_values = draft.hide_values;
    const types: NonNullable<AlertsRequest["types"]> = {};
    for (const type of alerts.types) {
      if (!type.available) continue;
      const now = draft.types[type.kind];
      const before = saved.types[type.kind];
      const change: { enabled?: boolean; threshold?: number } = {};
      if (now.enabled !== before.enabled) change.enabled = now.enabled;
      const value = parseDecimal(now.threshold);
      if (type.unit !== null && now.threshold !== before.threshold && value !== null) change.threshold = value;
      if (Object.keys(change).length > 0) types[type.kind] = change;
    }
    if (Object.keys(types).length > 0) body.types = types;
    return body;
  }, [alerts.types, draft, saved, hourError]);

  const dirty = Object.keys(changes).length > 0;
  const invalid = Object.keys(errors).length > 0 || hourError !== null;

  const edit = (patch: Partial<Omit<Draft, "types">>) => {
    setTouched(true);
    setDraft((current) => ({ ...current, ...patch }));
  };
  const editType = (kind: AlertKind, patch: Partial<Draft["types"][AlertKind]>) => {
    setTouched(true);
    setDraft((current) => ({ ...current, types: { ...current.types, [kind]: { ...current.types[kind], ...patch } } }));
  };

  const submit = () =>
    save.mutate(changes, {
      onSuccess: () => {
        setTouched(false);
        toast({ tone: "success", message: "Alertas salvos." });
      },
      onError: (error) => toast({ tone: "error", message: errorMessage(error) }),
    });

  const preview = useMemo(
    () =>
      sampleDigest(
        alerts.types.map((t) => ({ ...t, enabled: draft.types[t.kind].enabled })),
        draft.hide_values,
        money,
      ),
    [alerts.types, draft.types, draft.hide_values, money],
  );

  return (
    <Grid cols={2} alignStart>
      <div className="alerts__column">
        {telegram}
        <Panel className="alerts__card" title="O que avisar">
          <ul className="rule-list">
            {alerts.types.map((type) => {
              const text = KIND_TEXT[type.kind];
              const id = `${baseId}-${type.kind}`;
              const state = draft.types[type.kind];
              const error = errors[type.kind];
              return (
                <li key={type.kind} className="rule-row">
                  <div className="rule-row__main">
                    <div className="rule-row__sentence">
                      {type.unit === null || !text.lead ? (
                        <span className="rule-row__lead">{text.name}</span>
                      ) : (
                        <>
                          <span className="rule-row__lead">{text.lead}</span>
                          <span className="rule-row__field">
                            <input
                              id={`${id}-limit`}
                              className="text-input rule-row__input"
                              inputMode="decimal"
                              value={state.threshold}
                              aria-label={`Limite: ${text.name}`}
                              aria-invalid={error ? true : undefined}
                              aria-describedby={error ? `${id}-error` : undefined}
                              disabled={!type.available || !state.enabled}
                              onChange={(event) => editType(type.kind, { threshold: event.target.value })}
                            />
                          </span>
                          {text.tail && <span className="rule-row__tail">{text.tail}</span>}
                        </>
                      )}
                      <InfoTip label={`Quando avisa: ${text.name}`}>{text.why}</InfoTip>
                    </div>
                    {!type.available && text.unavailable && <p className="rule-row__note">{text.unavailable}</p>}
                    {error && state.enabled && (
                      <p id={`${id}-error`} className="rule-row__error" role="alert">
                        {error}
                      </p>
                    )}
                  </div>
                  <input
                    id={`${id}-switch`}
                    type="checkbox"
                    role="switch"
                    className="switch"
                    aria-label={kindName(type.kind)}
                    checked={type.available && state.enabled}
                    disabled={!type.available}
                    onChange={(event) => editType(type.kind, { enabled: event.target.checked })}
                  />
                </li>
              );
            })}
          </ul>
        </Panel>
      </div>

      <div className="alerts__column">
        <Panel className="alerts__card" title="Quando e com quê">
          <div className="field">
            <label className="field__label" htmlFor={`${baseId}-time`}>
              Hora do resumo (horário de Brasília)
            </label>
            <input
              id={`${baseId}-time`}
              type="time"
              className="text-input alerts__time"
              value={draft.send_at}
              aria-invalid={hourError ? true : undefined}
              aria-describedby={hourError ? `${baseId}-time-error` : `${baseId}-time-hint`}
              onChange={(event) => edit({ send_at: event.target.value })}
            />
            {hourError && (
              <p id={`${baseId}-time-error`} className="rule-row__error" role="alert">
                {hourError}
              </p>
            )}
            <p id={`${baseId}-time-hint`} className="field__hint">
              Uma vez por dia, só quando há algo a dizer.
            </p>
          </div>
          <div className="switch-row">
            <span className="switch-row__label">
              <span className="alerts__label-line">
                <label htmlFor={`${baseId}-hide`}>Sem valores no resumo</label>
                <InfoTip label="Por que tirar os valores do resumo" align="end">
                  O resumo passa pelos servidores do Telegram. Com esta opção ele leva só nomes e percentuais, nenhum
                  valor em reais. Isto vale para o que vai ao Telegram; o olho da barra só esconde a tela.
                </InfoTip>
              </span>
              <span className="field__hint">Só nomes e percentuais, nenhum valor em reais.</span>
            </span>
            <input
              id={`${baseId}-hide`}
              type="checkbox"
              role="switch"
              className="switch"
              checked={draft.hide_values}
              onChange={(event) => edit({ hide_values: event.target.checked })}
            />
          </div>
          <div className="alerts__actions">
            <MagneticButton disabled={!dirty || invalid} loading={save.isPending} onClick={submit}>
              Salvar alertas
            </MagneticButton>
            {dirty && (
              <button
                type="button"
                className="link-btn"
                onClick={() => {
                  setTouched(false);
                  setDraft(saved);
                }}
              >
                Descartar mudanças
              </button>
            )}
          </div>
        </Panel>

        <Panel
          className="alerts__card"
          title="Como o resumo fica"
          infoLabel="Sobre o exemplo do resumo"
          info="Exemplo com dados fictícios, só para mostrar o formato. Acompanha os tipos ligados e a escolha de sem valores."
        >
          {preview.length === 0 ? (
            <p>Nenhum tipo ligado: nada será enviado.</p>
          ) : (
            <div className="alerts__digest" role="group" aria-label="Exemplo do resumo">
              <p className="alerts__digest-title">Prumo – resumo de 25/11</p>
              {preview.map((section) => (
                <div key={section.title}>
                  <p className="alerts__digest-section">{section.title}</p>
                  {section.lines.map((line) => (
                    <p key={line}>{line}</p>
                  ))}
                </div>
              ))}
            </div>
          )}
        </Panel>
      </div>
    </Grid>
  );
}
