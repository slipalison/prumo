import type { CSSProperties } from "react";

type SkeletonProps = { height?: number | string; width?: number | string; radius?: number };

export function Skeleton({ height = 18, width = "100%", radius = 10 }: SkeletonProps) {
  const style: CSSProperties = { height, width, borderRadius: radius };
  return <span className="skeleton" style={style} aria-hidden="true" />;
}

/** O esqueleto de uma tela inteira, enquanto a carteira chega. */
export function ScreenSkeleton() {
  return (
    <div className="screen-skeleton" role="status" aria-label="Carregando">
      <Skeleton height={34} width="60%" />
      <Skeleton height={88} radius={22} />
      <div className="screen-skeleton__row">
        <Skeleton height={44} width={96} radius={999} />
        <Skeleton height={44} width={96} radius={999} />
        <Skeleton height={44} width={96} radius={999} />
      </div>
      <Skeleton height={180} radius={22} />
    </div>
  );
}
