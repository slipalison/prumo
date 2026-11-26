import { setupServer } from "msw/node";
import { allHandlers } from "./handlers";

export { getDb, resetDb } from "./handlers";

/** O banco falso servido aos testes (Node). Cada teste começa do zero (`setup.ts`). */
export const server = setupServer(...allHandlers);
