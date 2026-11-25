import { useState } from "react";
import { Providers } from "./Providers";
import { createQueryClient } from "./queryClient";
import { Shell } from "./Shell";

export function App() {
  const [client] = useState(createQueryClient);
  return (
    <Providers client={client}>
      <Shell />
    </Providers>
  );
}
