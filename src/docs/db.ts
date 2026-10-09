// Interface DbAdapter du cœur, branchée sur l'index de la documentation (base SQLite côté Rust).
import { invoke } from "@tauri-apps/api/core";
import { initSchema } from "./core/index.js";

export interface DbAdapter {
  exec(sql: string): Promise<void>;
  run(sql: string, params?: unknown[]): Promise<{ lastInsertId?: number; changes: number }>;
  all<T = Record<string, unknown>>(sql: string, params?: unknown[]): Promise<T[]>;
  transaction<T>(fn: () => Promise<T>): Promise<T>;
}

const db: DbAdapter = {
  exec: (sql) => invoke("docs_db_exec", { sql }),
  run: (sql, params = []) => invoke("docs_db_run", { sql, params }),
  all: (sql, params = []) => invoke("docs_db_all", { sql, params }),
  // Le cœur est écrit pour ne pas dépendre des transactions (hash enregistré en dernier :
  // une indexation interrompue est refaite au passage suivant).
  transaction: (fn) => fn(),
};

export interface IndexInfo {
  sqliteVersion: string;
  fts5: boolean;
  trigram: boolean;
  refMode: "trigram" | "like";
}

let opening: Promise<{ db: DbAdapter; info: IndexInfo }> | undefined;

/** Ouvre l'index une seule fois (schéma créé ou migré) et renvoie { db, info }. */
export function openIndex() {
  opening ??= (async () => {
    const info = (await initSchema(db, { log: (msg: string) => console.warn(msg) })) as IndexInfo;
    if (info.refMode !== "trigram") console.warn("[docs] trigram indisponible : recherche en mode LIKE");
    return { db, info };
  })();
  return opening;
}
