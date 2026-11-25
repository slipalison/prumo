import { useQueryClient } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useId, useState, type FormEvent } from "react";
import { errorMessage } from "../../api/client";
import { cashKeys } from "../../api/cashQueries";
import { incomeKeys } from "../../api/incomeQueries";
import { objectivesKeys } from "../../api/objectivesQueries";
import { useContributions, usePlan } from "../../api/queries";
import type { AssetClass, Plan, Portfolio } from "../../api/types";
import { GlassCard } from "../../components/GlassCard";
import { IconArrowRight, IconSparkle } from "../../components/icons";
import { MagneticButton } from "../../components/MagneticButton";
import { MoneyInput } from "../../components/MoneyInput";
import { Plumb } from "../../components/Plumb";
import { useToast } from "../../components/toastContext";
import { cx } from "../../lib/cx";
import { formatBRL } from "../../lib/format";
import { sumTargets, targetsFromClasses } from "../../lib/goals";
import { useMoney } from "../../lib/privacy";
import { needsRating, ratingIntro } from "../../lib/rating";
import { CashSuggestion } from "../cash/CashSuggestion";
import { PlanObjectives } from "../objectives/PlanObjectives";
import { Onboarding } from "../onboarding/Onboarding";
import { AddAssetSheet } from "../portfolio/AddAssetSheet";
import { RateAssetSheet, type RatingTarget } from "../portfolio/RateAssetSheet";
import { ContributionSheet } from "./ContributionSheet";
import { PlanResult } from "./PlanResult";

const QUICK_AMOUNTS = [500, 1000, 2000, 5000];

/** `R$ 500`, `R$ 1.000`: os atalhos fixos não são dado da pessoa, e aparecem sempre. */
const formatQuick = (value: number) => formatBRL(value).replace(",00", "");

type ContributeScreenProps = { portfolio: Portfolio };

export function ContributeScreen({ portfolio }: ContributeScreenProps) {
  const [onboarding, setOnboarding] = useState(false);
  const [rating, setRating] = useState<RatingTarget>(null);

  // O assistente fica fora do ramo: quando o primeiro ativo entra, a tela vira
  // a calculadora, e o assistente precisa continuar aberto para o passo final.
  return (
    <>
      {portfolio.assets.length === 0 ? (
        <div className="screen contribute">
          <GlassCard as="section" className="welcome" aria-labelledby="welcome-title">
            <Plumb state="idle" size={110} label="Prumo esperando a carteira" />
            <h1 id="welcome-title" className="welcome__title">
              Vamos colocar sua carteira no prumo
            </h1>
            <p className="welcome__text">
              Em três passos: seu perfil, o primeiro ativo, e pronto — a partir daí você só diz quanto vai aportar.
            </p>
            <MagneticButton onClick={() => setOnboarding(true)} className="wide">
              <IconSparkle /> Montar minha carteira
            </MagneticButton>
          </GlassCard>
        </div>
      ) : (
        <Calculator portfolio={portfolio} />
      )}
      <Onboarding
        open={onboarding}
        portfolio={portfolio}
        onClose={() => setOnboarding(false)}
        onRateAsset={(asset) => {
          setOnboarding(false);
          setRating({ asset, intro: ratingIntro(asset) });
        }}
      />
      <RateAssetSheet target={rating} portfolio={portfolio} onClose={() => setRating(null)} />
    </>
  );
}

function Calculator({ portfolio }: { portfolio: Portfolio }) {
  const titleId = useId();
  const inputId = useId();
  const [amount, setAmount] = useState(0);
  // Quanto do valor digitado é caixa (o que o aporte registrado vai baixar).
  const [cashIncluded, setCashIncluded] = useState(0);
  const [plan, setPlan] = useState<Plan | null>(null);
  const [confirming, setConfirming] = useState(false);
  const [registered, setRegistered] = useState(false);
  const [rating, setRating] = useState<RatingTarget>(null);
  const [adding, setAdding] = useState<AssetClass | null>(null);
  const planMutation = usePlan();
  const contributions = useContributions();
  const toast = useToast();
  const money = useMoney();
  const queryClient = useQueryClient();

  const lastAmount = contributions.data?.[0]?.amount;
  const quick = lastAmount && !QUICK_AMOUNTS.includes(lastAmount) ? [...QUICK_AMOUNTS, lastAmount] : QUICK_AMOUNTS;
  const noGoals = sumTargets(targetsFromClasses(portfolio.classes)) === 0;

  const calculate = (value: number = amount) => {
    if (value <= 0 || planMutation.isPending) return;
    setRegistered(false);
    planMutation.mutate(value, {
      onSuccess: setPlan,
      onError: (error) => toast({ tone: "error", message: errorMessage(error) }),
    });
  };

  // O valor muda por digitação, chip ou caixa incluído: o plano na tela deixa
  // de valer em qualquer um dos três. Valor menor que o caixa incluído desfaz
  // a inclusão: o caixa nunca passa do aporte.
  const changeAmount = (value: number, included: number = cashIncluded) => {
    setAmount(value);
    setCashIncluded(value >= included ? included : 0);
    if (plan) setPlan(null);
  };

  const submit = (event: FormEvent) => {
    event.preventDefault();
    calculate();
  };

  // Registrado o aporte, o "Já comprei" que abriu a folha está saindo de cena
  // (ainda no DOM, animando): a folha devolveria o foco a um botão que some em
  // seguida. Quem recebe o foco é o campo de valor — o próximo aporte começa ali.
  useEffect(() => {
    if (registered) document.getElementById(inputId)?.focus({ preventScroll: true });
  }, [registered, inputId]);

  const plumbState = planMutation.isPending ? "swinging" : registered ? "success" : plan ? "settled" : "idle";

  return (
    <div className="screen contribute">
      <GlassCard as="section" className="ask" aria-labelledby={titleId}>
        <div className="ask__plumb">
          <Plumb state={plumbState} size={58} label={planMutation.isPending ? "Equilibrando a carteira" : "Prumo"} />
        </div>
        <form className="ask__form" onSubmit={submit}>
          <h1 id={titleId} className="ask__title">
            Quanto você vai aportar?
          </h1>
          <MoneyInput
            id={inputId}
            labelledBy={titleId}
            size="hero"
            value={amount}
            onChange={changeAmount}
            focusOnMount
            screenFocus
            onEnter={() => calculate()}
          />
          <CashSuggestion amount={amount} included={cashIncluded} onChange={changeAmount} />
          <div className="quick" role="group" aria-label="Valores rápidos">
            {quick.map((value) => (
              <motion.button
                key={value}
                type="button"
                className={cx("chip chip--quick", value === amount && "is-active")}
                whileTap={{ scale: 0.94 }}
                onClick={() => {
                  setAmount(value);
                  setCashIncluded((current) => (value >= current ? current : 0));
                  calculate(value);
                }}
              >
                {value === lastAmount ? `Último: ${money.brl(value)}` : formatQuick(value)}
              </motion.button>
            ))}
          </div>
          <MagneticButton type="submit" disabled={amount <= 0} loading={planMutation.isPending} className="ask__submit">
            {planMutation.isPending ? "Equilibrando…" : "Calcular"}
            {!planMutation.isPending && <IconArrowRight />}
          </MagneticButton>
        </form>
        {noGoals && (
          <p className="ask__hint">
            Você ainda não tem metas. <a href="#/ajustes/metas">Defina em Metas</a> para eu saber para onde levar a carteira.
          </p>
        )}
      </GlassCard>

      <AnimatePresence mode="wait">
        {registered && (
          <motion.div
            key="registered"
            className="registered glass"
            role="status"
            initial={{ opacity: 0, scale: 0.96 }}
            animate={{ opacity: 1, scale: 1 }}
            exit={{ opacity: 0 }}
          >
            <Plumb state="success" size={72} label="Aporte registrado" />
            <div>
              <h2 className="registered__title">Aporte registrado!</h2>
              <p className="field__hint">A carteira já está atualizada. Até o próximo.</p>
            </div>
          </motion.div>
        )}
        {plan && !registered && (
          <motion.div key="plan" initial={{ opacity: 0, y: 16 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: -8 }}>
            <PlanResult
              plan={plan}
              assets={portfolio.assets}
              onConfirm={() => setConfirming(true)}
              onRateAsset={(asset) => setRating({ asset, intro: ratingIntro(asset) })}
              onEditAsset={(asset) => setRating({ asset })}
              onAddAsset={setAdding}
            />
            <PlanObjectives plan={plan} />
          </motion.div>
        )}
      </AnimatePresence>

      <RateAssetSheet
        target={rating}
        portfolio={portfolio}
        onClose={() => setRating(null)}
        // Com nota nova, o plano na tela ficou velho: refaz com o mesmo valor.
        onSaved={() => {
          if (plan) calculate(plan.amount);
        }}
      />

      {/* O aviso "classe sem ativo" cadastra aqui mesmo: quem nasce sem nota
          segue para o checklist; quem já tem nota refaz o plano na hora. */}
      <AddAssetSheet
        open={adding !== null}
        initialClass={adding ?? undefined}
        onClose={() => setAdding(null)}
        onCreated={(asset) => {
          setAdding(null);
          if (needsRating(asset)) setRating({ asset, intro: ratingIntro(asset) });
          else if (plan) calculate(plan.amount);
        }}
      />

      {plan && (
        <ContributionSheet
          open={confirming}
          plan={plan}
          cashUsed={Math.min(cashIncluded, plan.amount)}
          onClose={() => setConfirming(false)}
          onRegistered={() => {
            setConfirming(false);
            setRegistered(true);
            setPlan(null);
            setAmount(0);
            setCashIncluded(0);
            // O aporte baixou o caixa que usou: o saldo e a renda se refazem.
            void queryClient.invalidateQueries({ queryKey: cashKeys.all });
            void queryClient.invalidateQueries({ queryKey: incomeKeys.all });
            // O aporte entra na média e no valor dos objetivos.
            void queryClient.invalidateQueries({ queryKey: objectivesKeys.all });
            toast({ tone: "success", message: "Aporte registrado. A carteira já está atualizada." });
          }}
        />
      )}
    </div>
  );
}
