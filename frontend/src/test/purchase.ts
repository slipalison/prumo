import { screen } from "@testing-library/react";
import type { UserEvent } from "@testing-library/user-event";

/**
 * No plano na tela: marca como sugerido o que ainda não está marcado e abre a
 * conferência "Registrar aporte" (o caminho de quem comprou o que o plano pediu).
 */
export async function openRegistration(user: UserEvent): Promise<HTMLElement> {
  const register = await screen.findByRole("button", { name: /Registrar .*comprad/ });
  const rest = screen.queryByRole("button", { name: /como sugerido/ });
  if (rest) await user.click(rest);
  await user.click(register);
  return screen.findByRole("dialog", { name: "Registrar aporte" });
}
