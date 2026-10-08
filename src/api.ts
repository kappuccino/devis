import { invoke } from "@tauri-apps/api/core";

export type Family = "CFA" | "CFO";

export interface Product {
  ref: string;
  enedis_code: string | null;
  designation: string;
  public_price: number;
  eco_tax: number | null;
  eco_tax_code: string | null;
  family: Family | null;
  threshold_price: number | null;
}

export interface ProductHit {
  ref: string;
  designation: string;
}

export interface Client {
  code: string;
  name: string;
  group_name: string | null;
  subgroup: string | null;
  sales_rep: string | null;
  discount_cfa: number;
  discount_cfo: number;
  email: string | null;
  franco: string | null;
  siren: string | null;
  price_lists: string[];
}

/** Conditions de prix d'un devis : copiées du client à sa sélection, modifiables sur le devis. */
export interface PricingContext {
  price_lists: string[];
  discount_cfa: number;
  discount_cfo: number;
  /** Liste forcée (parmi les favorites), prioritaire sur les autres ; null si aucune. */
  forced_price_list: string | null;
}

export interface PriceList {
  code: string;
  label: string | null;
  item_count: number;
  client_count: number;
}

export interface PriceListItem {
  product_ref: string;
  designation: string | null;
  price: number;
  label: string | null;
  public_price: number | null;
  family: Family | null;
}

export interface ClientRef {
  code: string;
  name: string;
}

export interface ResolvedPrice {
  product_ref: string;
  enedis_code: string | null;
  designation: string;
  unit_price: number;
  source: string;
  family: Family | null;
  public_price: number | null;
  threshold_price: number | null;
}

export interface QuoteSummary {
  id: number;
  number: string;
  /** null : client ponctuel (n'existe que dans le devis). */
  client_code: string | null;
  client_name: string;
  date: string;
  total_ht: number;
  /** Total après la remise globale. */
  total_net: number;
  line_count: number;
}

/**
 * `item` : article ; `text` : texte libre (dans `designation`) ;
 * `subtotal` : sous-total des articles depuis le sous-total précédent (libellé dans `designation`).
 */
export interface QuoteLine {
  kind: "item" | "text" | "subtotal";
  product_ref: string;
  /** Code ENEDIS, copié du catalogue à la saisie de la ligne. */
  enedis_code: string | null;
  designation: string;
  /** Vide (null) tant qu'elle n'a pas été saisie ; enregistrée à 0. */
  quantity: number | null;
  unit_price: number;
  /** Remise supplémentaire de la ligne, en %. */
  discount: number;
  /** Ligne en option : hors total HT, comptée dans « Total options ». */
  is_option: boolean;
  price_source: string | null;
  public_price: number | null;
  /** Affiché dans l'appli, jamais sur le PDF. */
  threshold_price: number | null;
}

export interface Quote extends PricingContext {
  id: number | null;
  number: string | null;
  /** null : client ponctuel (n'existe que dans le devis). */
  client_code: string | null;
  client_name: string;
  date: string;
  notes: string | null;
  /** Somme des lignes (remises de ligne comprises). */
  total_ht: number;
  /** Remise globale sur le total, en %. */
  discount_pct: number;
  /** Total après la remise globale. */
  total_net: number;
  /** Total des lignes en option (hors total HT). */
  total_options: number;
  lines: QuoteLine[];
}

export interface ImportReport {
  products: number;
  clients: number;
  price_lists: number;
  price_list_items: number;
  client_links: number;
  warnings: string[];
}

export interface DbStats {
  path: string;
  products: number;
  clients: number;
  price_lists: number;
  price_list_items: number;
  quotes: number;
}

export type Settings = Record<string, string>;

export const api = {
  listProducts: () => invoke<Product[]>("list_products"),
  searchProducts: (query: string, priceLists: string[]) =>
    invoke<ProductHit[]>("search_products", { query, priceLists }),

  listClients: () => invoke<Client[]>("list_clients"),
  getClient: (code: string) => invoke<Client | null>("get_client", { code }),
  updateClient: (code: string, discountCfa: number, discountCfo: number, priceLists: string[]) =>
    invoke<void>("update_client", { code, discountCfa, discountCfo, priceLists }),

  listPriceLists: () => invoke<PriceList[]>("list_price_lists"),
  getPriceListItems: (code: string) => invoke<PriceListItem[]>("get_price_list_items", { code }),
  getPriceListClients: (code: string) => invoke<ClientRef[]>("get_price_list_clients", { code }),

  resolvePrice: (pricing: PricingContext, productRef: string) =>
    invoke<ResolvedPrice>("resolve_price", { pricing, productRef }),

  listQuotes: () => invoke<QuoteSummary[]>("list_quotes"),
  getQuote: (id: number) => invoke<Quote>("get_quote", { id }),
  saveQuote: (quote: Quote) => invoke<Quote>("save_quote", { quote }),
  deleteQuote: (id: number) => invoke<void>("delete_quote", { id }),
  duplicateQuote: (id: number, date: string) => invoke<Quote>("duplicate_quote", { id, date }),

  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),
  dbStats: () => invoke<DbStats>("db_stats"),
  importLpn: (path: string) => invoke<ImportReport>("import_lpn", { path }),
  saveFile: (path: string, contents: Uint8Array) =>
    invoke<void>("save_file", { path, contents: Array.from(contents) }),
};
