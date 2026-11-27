import type { Portfolio } from "../../api/types";
import { Grid } from "../../components/Layout";
import { CompositionPanel } from "./CompositionPanel";
import { TodayPanel } from "./TodayPanel";

/** A composição e o dia da carteira, lado a lado (a composição pede a largura). */
export function PanelDetails({ portfolio }: { portfolio: Portfolio }) {
  return (
    <Grid cols="main-aside" alignStart className="panel-details">
      <CompositionPanel assets={portfolio.assets} />
      <TodayPanel assets={portfolio.assets} />
    </Grid>
  );
}
