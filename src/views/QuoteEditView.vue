<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { useRouter } from "vue-router";
import AutoComplete, { type AutoCompleteCompleteEvent } from "primevue/autocomplete";
import InputText from "primevue/inputtext";
import Textarea from "primevue/textarea";
import Button from "primevue/button";
import Tag from "primevue/tag";
import { useToast } from "primevue/usetoast";
import Dialog from "primevue/dialog";
import InputNumber from "primevue/inputnumber";
import Select from "primevue/select";
import {
  api,
  type Client,
  type PriceList,
  type PricingContext,
  type ProductHit,
  type Quote,
  type QuoteContact,
  type QuoteLine,
} from "../api";
import { errorMessage, formatEuro, formatUnitPrice, MOD, round2, todayIso } from "../format";
import { exportQuotePdf } from "../composables/usePdf";
import QuoteDocsDialog from "../components/docs/QuoteDocsDialog.vue";
import { readDraft, removeDraft, writeDraft, type QuoteDraft } from "../drafts";
import { useConfirm } from "primevue/useconfirm";
import { useLineClipboard } from "../composables/useLineClipboard";
import {
  FEE_KINDS,
  insertBlock,
  isFee,
  lineTotal,
  moveBlockBefore,
  netUnitPrice,
  quoteTotals,
  subtotals,
  type FeeKind,
} from "../quoteLines";
import { favoriteLists } from "../favorites";
import { autoFeeChanges, feeRulesFrom, type FeeRules } from "../fees";

const props = defineProps<{ id?: string }>();
const router = useRouter();
const toast = useToast();
const confirm = useConfirm();

/** Ligne en cours d'édition : la ligne enregistrée + ce qui sert seulement à l'affichage. */
interface EditLine extends QuoteLine {
  key: number;
  resolvedRef: string | null;
  /** Code ENEDIS du produit trouvé (pour savoir si la colonne ENEDIS a été modifiée). */
  resolvedEnedis?: string | null;
  /** Réf en cours de résolution : évite un double appel (sélection + Entrée + blur). */
  pendingRef: string | null;
  error: string | null;
  /**
   * Texte des champs quantité, prix et remise, tel que tapé. `quantity` / `unit_price` / `discount`
   * en sont déduits à chaque touche : totaux et sous-totaux suivent la frappe, sans réécrire le champ.
   */
  qtyText: string;
  priceText: string;
  discountText: string;
  /** Frais ajouté automatiquement (conditions de vente), tant qu'on ne l'a pas modifié. */
  auto?: boolean;
}

let nextKey = 0;
const blankLine = (): EditLine => ({
  key: nextKey++,
  kind: "item",
  product_ref: "",
  designation: "",
  quantity: null,
  unit_price: 0,
  discount: 0,
  is_option: false,
  enedis_code: null,
  price_source: null,
  public_price: null,
  threshold_price: null,
  discounted_price: null,
  lpn_price: null,
  lpn_list: null,
  resolvedRef: null,
  pendingRef: null,
  error: null,
  qtyText: "",
  priceText: "",
  discountText: "",
});

/** « 1 234,5 » → 1234.5 ; vide ou illisible → null. */
function parseNumber(text: string): number | null {
  const t = text.replace(/[\s\u00a0\u202f€]/g, "").replace(",", ".");
  if (!t) return null;
  const v = Number(t);
  return Number.isFinite(v) ? v : null;
}

/** Valeur affichée dans un champ en cours d'édition (sans séparateur de milliers). */
const editText = (v: number | null, decimals: 2 | 3) =>
  v == null
    ? ""
    : v.toLocaleString("fr-FR", {
        useGrouping: false,
        minimumFractionDigits: decimals === 2 ? 2 : 0,
        maximumFractionDigits: decimals,
      });

/** Ligne chargée (devis, collage) : on prépare le texte de ses champs. */
const withTexts = <T extends QuoteLine>(l: T) => ({
  ...l,
  // Le code ENEDIS affiché correspond déjà au produit : le quitter ne relance pas de recherche.
  resolvedEnedis: l.enedis_code,
  qtyText: editText(l.quantity, 3),
  priceText: editText(l.unit_price, 2),
  discountText: l.discount ? editText(l.discount, 3) : "",
});

function onQtyInput(line: EditLine, text: string) {
  line.qtyText = text;
  line.quantity = parseNumber(text);
}

function onPriceInput(line: EditLine, text: string) {
  line.priceText = text;
  line.unit_price = parseNumber(text) ?? 0;
}

/** Remise supplémentaire de la ligne, en % (0 à 100). */
function onDiscountInput(line: EditLine, text: string) {
  line.discountText = text;
  line.discount = Math.min(100, Math.max(0, parseNumber(text) ?? 0));
}

function onPriceBlur(line: EditLine) {
  line.unit_price = round2(line.unit_price);
  line.priceText = editText(line.unit_price, 2);
}

const quoteId = ref<number | null>(null);
const number = ref<string | null>(null);
const date = ref(todayIso());
const notes = ref("");
const lines = ref<EditLine[]>([blankLine()]);

const clients = ref<Client[]>([]);
const allPriceLists = ref<PriceList[]>([]);
/** Client de la base choisi dans la recherche (null : aucun, ou client ponctuel). */
const client = ref<Client | null>(null);
const clientSuggestions = ref<Client[]>([]);

// Client et conditions de prix du devis : copiés du client à sa sélection, puis modifiables
// sur ce devis seulement (la fiche client n'est pas touchée). Code null : client ponctuel.
const clientCode = ref<string | null>(null);
const clientName = ref("");
const discountCfa = ref(0);
const discountCfo = ref(0);
const priceLists = ref<string[]>([]);
/** Liste de prix forcée sur ce devis (parmi les favorites) : prioritaire sur les autres. */
const forcedList = ref<string | null>(null);
const hasClient = computed(() => clientName.value.trim() !== "");
const pricing = computed<PricingContext>(() => ({
  price_lists: [...priceLists.value],
  discount_cfa: discountCfa.value ?? 0,
  discount_cfo: discountCfo.value ?? 0,
  forced_price_list: forcedList.value,
}));
/** Toutes les listes où chercher un produit (forcée comprise). */
const searchLists = computed(() => [...priceLists.value, ...(forcedList.value ? [forcedList.value] : [])]);

// Listes favorites (Réglages) proposées pour forcer une liste ; la liste forcée actuelle reste
// proposée même si elle a été retirée des favorites depuis.
const favorites = ref<string[]>([]);
const forcedOptions = computed(() => {
  const codes = [...favorites.value];
  if (forcedList.value && !codes.includes(forcedList.value)) codes.push(forcedList.value);
  return codes.map((code) => {
    const label = allPriceLists.value.find((l) => l.code === code)?.label;
    return { code, label: label ? `${code} — ${label}` : code };
  });
});

function onForcedListChange() {
  repriceLines();
}
const pricingKey = computed(() => JSON.stringify(pricing.value));
const productSuggestions = ref<ProductHit[]>([]);
const saving = ref(false);

// Frais de port / de facturation : à part des autres lignes, toujours en bas du devis
// (pas de déplacement ni de sélection), un seul de chaque.
const feeLines = ref<EditLine[]>([]);
const FEE_LABELS: Record<FeeKind, string> = { shipping: "Frais de port", billing: "Frais de facturation" };
const hasFee = (kind: FeeKind) => feeLines.value.some((l) => l.kind === kind);

function addFee(kind: FeeKind, auto?: { amount: number }) {
  if (hasFee(kind)) return;
  const fee: EditLine = { ...blankLine(), kind, quantity: 1, designation: FEE_LABELS[kind] };
  if (auto) Object.assign(fee, { auto: true, unit_price: auto.amount, priceText: editText(auto.amount, 2) });
  // Ordre fixe : frais de port, puis frais de facturation.
  feeLines.value = [...feeLines.value, fee].sort((a, b) => FEE_KINDS.indexOf(a.kind as FeeKind) - FEE_KINDS.indexOf(b.kind as FeeKind));
  if (!auto) nextTick(() => document.getElementById(`fee-${fee.key}`)?.focus());
}

function removeFee(fee: EditLine) {
  feeLines.value = feeLines.value.filter((l) => l !== fee);
  // Retiré à la main : on ne le repropose plus sur ce devis.
  dismissedFees.add(fee.kind as FeeKind);
}

// Frais automatiques (seuils dans Réglages → Config PDF) : ajoutés / retirés quand le total des
// produits passe sous / au-dessus du seuil. Un frais modifié ou retiré à la main n'est plus touché.
const feeRules = ref<FeeRules>({});
const dismissedFees = new Set<FeeKind>();

function applyAutoFees() {
  const present = feeLines.value.map((l) => ({ kind: l.kind as FeeKind, auto: !!l.auto }));
  const { add, remove } = autoFeeChanges(totals.value.products, feeRules.value, present, dismissedFees);
  if (remove.length) feeLines.value = feeLines.value.filter((l) => !remove.includes(l.kind as FeeKind));
  for (const kind of add) addFee(kind, { amount: feeRules.value[kind]!.amount });
}

/** Remise globale sur les produits, en % (les frais ne sont pas remisés). */
const globalDiscount = ref(0);
/** Total HT (produits hors options + frais), remise, total remisé et total des options. */
const totals = computed(() => quoteTotals([...lines.value, ...feeLines.value], globalDiscount.value));
// Frais automatiques : seulement sur les modifications du devis, pas à son chargement.
watch(
  () => totals.value.products,
  () => {
    if (draftReady.value) applyAutoFees();
  },
);
const total = computed(() => totals.value.total);
const netTotal = computed(() => totals.value.net);
const filledLines = computed(() => lines.value.filter((l) => l.kind === "item" && l.product_ref.trim()));
/** Montant de chaque sous-total, par clé de ligne (recalculé à chaque frappe et déplacement). */
const subtotalAmounts = computed(() => {
  const amounts = subtotals(lines.value);
  return new Map(lines.value.map((l) => [l.key, amounts.get(l)]));
});

/**
 * Navigation entre les quantités, lignes de texte et sous-totaux sautés :
 * - Entrée / ↓ : quantité de l'article suivant ; après le dernier, Entrée revient à la saisie de réf ;
 * - ↑ : quantité de l'article précédent.
 */
function moveToQuantity(line: EditLine, direction: 1 | -1, e?: Event) {
  e?.preventDefault();
  line.qtyText = editText(line.quantity, 3);
  const items = lines.value.filter((l) => l.kind === "item" && !isEntryLine(l));
  const target = items[items.indexOf(line) + direction];
  let input = target ? document.getElementById(`qty-${target.key}`) : null;
  if (!target && direction === 1 && e instanceof KeyboardEvent && e.key === "Enter") {
    input = document.getElementById(`ref-${lines.value[lines.value.length - 1].key}`);
  }
  if (input instanceof HTMLInputElement) {
    input.focus();
    input.select();
  }
}
const isBlankItem = (l: EditLine) => l.kind === "item" && !l.product_ref;
/** La dernière ligne est la ligne de saisie : ni déplaçable, ni enregistrée tant qu'elle est vide. */
const isEntryLine = (l: EditLine) => l === lines.value[lines.value.length - 1] && isBlankItem(l);

/** Tab dans la recherche client sans aucune correspondance : on crée un client ponctuel avec ce nom. */
function onClientTab(e: KeyboardEvent) {
  const typed = (e.target as HTMLInputElement).value.trim();
  if (!typed || typed === client.value?.name) return;
  const q = typed.toLowerCase();
  if (clients.value.some((c) => c.name.toLowerCase().includes(q) || c.code.toLowerCase().includes(q))) return;
  e.preventDefault();
  openEphemeral(typed);
}

/** Choix d'un client de la base : on copie ses conditions dans le devis et on recalcule les prix. */
/** Contact chez le client et commercial : propres au devis. */
const emptyContact = (): QuoteContact => ({ contact_name: "", contact_email: "", contact_phone: "", sales_rep: "" });
const contact = ref<QuoteContact>(emptyContact());

async function applyClient(c: Client) {
  clientCode.value = c.code;
  clientName.value = c.name;
  // Nouveau client : contact vide, commercial repris de la fiche client.
  contact.value = { ...emptyContact(), sales_rep: c.sales_rep ?? "" };
  discountCfa.value = c.discount_cfa;
  discountCfo.value = c.discount_cfo;
  priceLists.value = [...c.price_lists];
  await repriceLines();
}

// Client ponctuel : n'existe que dans ce devis (nom, remises, une liste de prix au plus).
const lastClientQuery = ref("");
const ephemeralVisible = ref(false);
const ephemeral = ref({ name: "", cfa: 0, cfo: 0, list: null as string | null });
const priceListOptions = computed(() =>
  allPriceLists.value.map((l) => ({ code: l.code, label: `${l.code}${l.label ? ` — ${l.label}` : ""}` })),
);

function openEphemeral(name?: string) {
  ephemeral.value = {
    name: name ?? (clientCode.value ? lastClientQuery.value : clientName.value || lastClientQuery.value),
    cfa: discountCfa.value,
    cfo: discountCfo.value,
    list: !clientCode.value ? (priceLists.value[0] ?? null) : null,
  };
  ephemeralVisible.value = true;
}

async function applyEphemeral() {
  const { name, cfa, cfo, list } = ephemeral.value;
  if (!name.trim()) {
    toast.add({ severity: "warn", summary: "Indiquez la raison sociale", life: 2500 });
    return;
  }
  ephemeralVisible.value = false;
  if (clientCode.value) contact.value = emptyContact();
  client.value = null;
  clientCode.value = null;
  clientName.value = name.trim();
  discountCfa.value = cfa ?? 0;
  discountCfo.value = cfo ?? 0;
  priceLists.value = list ? [list] : [];
  await repriceLines();
}

/** Liste de prix unique d'un client ponctuel, modifiable dans l'en-tête du devis. */
const ephemeralList = computed({
  get: () => priceLists.value[0] ?? null,
  set: (code: string | null) => {
    priceLists.value = code ? [code] : [];
    repriceLines();
  },
});

function searchClients(e: AutoCompleteCompleteEvent) {
  lastClientQuery.value = e.query.trim();
  const q = e.query.trim().toLowerCase();
  clientSuggestions.value = clients.value
    .filter((c) => c.name.toLowerCase().includes(q) || c.code.toLowerCase().includes(q))
    .slice(0, 50);
}

/**
 * La recherche part 200 ms après la frappe : si la réf a déjà été validée (Entrée) ou si le curseur
 * est passé ailleurs, on n'ouvre pas la liste (une liste vide la referme, cf. show-empty-message).
 */
const isStale = (line: EditLine) =>
  line.pendingRef != null ||
  line.product_ref.trim() === line.resolvedRef ||
  document.activeElement?.id !== `ref-${line.key}`;

async function searchProducts(e: AutoCompleteCompleteEvent, line: EditLine) {
  if (isStale(line)) {
    productSuggestions.value = [];
    return;
  }
  try {
    const hits = await api.searchProducts(e.query, searchLists.value);
    productSuggestions.value = isStale(line) ? [] : hits;
  } catch {
    productSuggestions.value = [];
  }
}

/** La dernière ligne est toujours vide : on y place le curseur pour enchaîner la saisie. */
/** `field` : on reste dans la colonne utilisée pour saisir (référence ou code ENEDIS). */
async function focusNewLine(field: "ref" | "enedis" = "ref") {
  const last = lines.value[lines.value.length - 1];
  await nextTick();
  document.getElementById(`${field}-${last.key}`)?.focus();
}

async function resolve(line: EditLine, focusNext = false, field: "ref" | "enedis" = "ref") {
  const r = line.product_ref.trim();
  if (r && r === line.pendingRef) return;
  line.product_ref = r;
  if (!r) {
    Object.assign(line, {
      designation: "",
      unit_price: 0,
      priceText: "",
      price_source: null,
      resolvedRef: null,
      public_price: null,
      threshold_price: null,
      discounted_price: null,
      lpn_price: null,
      lpn_list: null,
      error: null,
    });
    return;
  }
  if (!hasClient.value) {
    line.error = "Choisissez d'abord un client";
    return;
  }
  line.pendingRef = r;
  try {
    const p = await api.resolvePrice(pricing.value, r);
    Object.assign(line, {
      product_ref: p.product_ref,
      enedis_code: p.enedis_code,
      designation: p.designation,
      unit_price: p.unit_price,
      priceText: editText(p.unit_price, 2),
      price_source: p.source,
      public_price: p.public_price,
      threshold_price: p.threshold_price,
      discounted_price: p.discounted_price,
      lpn_price: p.lpn_price,
      lpn_list: p.lpn_list,
      resolvedRef: p.product_ref,
      resolvedEnedis: p.enedis_code,
      error: null,
    });
  } catch (e) {
    Object.assign(line, {
      resolvedRef: r,
      price_source: null,
      public_price: null,
      threshold_price: null,
      discounted_price: null,
      lpn_price: null,
      lpn_list: null,
      error: errorMessage(e),
    });
  } finally {
    line.pendingRef = null;
  }
  if (lines.value[lines.value.length - 1] === line) lines.value.push(blankLine());
  // En cas d'erreur, le curseur reste sur la ligne à corriger.
  if (focusNext && !line.error) focusNewLine(field);
}

function onRefBlur(line: EditLine) {
  if (line.product_ref.trim() !== line.resolvedRef) resolve(line);
}

/** Choix dans le sélecteur ou Entrée : on valide la réf puis on passe à une nouvelle ligne. */
function commitRef(line: EditLine) {
  if (line.product_ref.trim() !== line.resolvedRef) resolve(line, true);
  else if (line.product_ref && !line.error) focusNewLine();
}

/**
 * Saisie par code ENEDIS (colonne ENEDIS) : le serveur retrouve le produit qui porte ce code
 * (avec ou sans les points). Entrée valide et passe à la colonne ENEDIS d'une nouvelle ligne.
 */
function commitEnedis(line: EditLine, focusNext: boolean) {
  const code = (line.enedis_code ?? "").trim();
  if (!code) return;
  const unchanged = line.resolvedRef != null && code === line.resolvedEnedis;
  if (unchanged) {
    if (focusNext && !line.error) focusNewLine("enedis");
    return;
  }
  line.product_ref = code;
  resolve(line, focusNext, "enedis");
}

/**
 * Recalcule les prix avec les conditions du devis.
 * `onlyPublic` : seulement les lignes au prix public remisé (après un changement de remise),
 * pour ne pas écraser les prix de liste ni les prix saisis à la main.
 */
async function repriceLines(onlyPublic = false) {
  const priced = filledLines.value.filter((l) => !onlyPublic || l.price_source?.startsWith("Public"));
  if (!priced.length) return;
  // La réf est déjà connue : on force un nouveau calcul.
  priced.forEach((l) => (l.resolvedRef = null));
  await Promise.all(priced.map((l) => resolve(l)));
  toast.add({ severity: "info", summary: "Prix recalculés", life: 2000 });
}

/** Ajoute un titre, une ligne de texte ou un sous-total en fin de devis ; on la déplace ensuite à la poignée. */
function insertSpecialLine(kind: "title" | "text" | "subtotal") {
  const added: EditLine = { ...blankLine(), kind, quantity: 0, designation: kind === "subtotal" ? "Sous-total" : "" };
  lines.value = insertBlock(lines.value, [added], null, entryLine());
  if (!entryLine()) lines.value.push(blankLine());
  if (kind !== "subtotal") nextTick(() => document.getElementById(`text-${added.key}`)?.focus());
}

// ---------- Glisser-déposer ----------
// Fait main (pointeur + position des lignes) : les bibliothèques testées géraient mal les lignes
// de tableau dans la fenêtre de l'appli. Si la ligne attrapée est sélectionnée, toute la sélection suit.

const tbodyEl = ref<HTMLElement>();
const drag = ref<{ keys: Set<number>; startY: number; moved: boolean; targetKey: number | null } | null>(null);

function startDrag(line: EditLine, e: PointerEvent) {
  if (e.button !== 0) return;
  e.preventDefault();
  (document.activeElement as HTMLElement | null)?.blur?.();
  const block = selectedKeys.value.has(line.key) ? selectionBlock() : [line];
  drag.value = { keys: new Set(block.map((l) => l.key)), startY: e.clientY, moved: false, targetKey: null };
  document.body.classList.add("dragging-lines");
  window.addEventListener("pointermove", onDragMove);
  window.addEventListener("pointerup", endDrag);
  window.addEventListener("pointercancel", stopDrag);
}

/** Ligne devant laquelle on lâcherait : la première dont le milieu est sous le pointeur (null = fin). */
function onDragMove(e: PointerEvent) {
  const d = drag.value;
  if (!d || !tbodyEl.value) return;
  if (!d.moved && Math.abs(e.clientY - d.startY) < 4) return;
  d.moved = true;
  const rows = Array.from(tbodyEl.value.querySelectorAll<HTMLElement>("tr[data-key]"));
  const row = rows.find((r) => {
    const box = r.getBoundingClientRect();
    return e.clientY < box.top + box.height / 2;
  });
  d.targetKey = row ? Number(row.dataset.key) : null;
  // Défilement automatique près des bords de la zone de contenu.
  const scroller = tbodyEl.value.closest(".content");
  if (scroller) {
    const box = scroller.getBoundingClientRect();
    if (e.clientY < box.top + 48) scroller.scrollBy(0, -16);
    else if (e.clientY > box.bottom - 48) scroller.scrollBy(0, 16);
  }
}

function stopDrag() {
  drag.value = null;
  document.body.classList.remove("dragging-lines");
  window.removeEventListener("pointermove", onDragMove);
  window.removeEventListener("pointerup", endDrag);
  window.removeEventListener("pointercancel", stopDrag);
}

function endDrag() {
  const d = drag.value;
  stopDrag();
  if (!d?.moved) return;
  const block = lines.value.filter((l) => d.keys.has(l.key));
  const target = lines.value.find((l) => l.key === d.targetKey) ?? null;
  lines.value = moveBlockBefore(lines.value, block, target, entryLine());
}

onBeforeUnmount(stopDrag);

/** Trait d'insertion : sur la ligne devant laquelle on lâcherait (la ligne de saisie pour la fin). */
const dropKey = computed(() => {
  const d = drag.value;
  if (!d?.moved) return null;
  const key = d.targetKey ?? entryLine()?.key ?? null;
  return key != null && !d.keys.has(key) ? key : null;
});

// ---------- Sélection, copier / coller ----------

const { clipboard, copy, holdsLines } = useLineClipboard();
const selectedKeys = ref(new Set<number>());
let lastClickedKey: number | null = null;

const selectableLines = computed(() => lines.value.filter((l) => !isEntryLine(l)));
const selectedLines = computed(() => lines.value.filter((l) => selectedKeys.value.has(l.key)));
const allSelected = computed(
  () => selectableLines.value.length > 0 && selectableLines.value.every((l) => selectedKeys.value.has(l.key)),
);
const entryLine = () => {
  const last = lines.value[lines.value.length - 1];
  return last && isBlankItem(last) ? last : null;
};

/** Clic sur une case : Maj+clic sélectionne toute la plage depuis le clic précédent. */
function onSelectClick(line: EditLine, e: MouseEvent) {
  const keys = new Set(selectedKeys.value);
  const check = !keys.has(line.key);
  const from = lastClickedKey == null ? -1 : selectableLines.value.findIndex((l) => l.key === lastClickedKey);
  if (e.shiftKey && from >= 0) {
    const to = selectableLines.value.indexOf(line);
    const [a, b] = from < to ? [from, to] : [to, from];
    selectableLines.value.slice(a, b + 1).forEach((l) => (check ? keys.add(l.key) : keys.delete(l.key)));
  } else if (check) {
    keys.add(line.key);
  } else {
    keys.delete(line.key);
  }
  lastClickedKey = line.key;
  selectedKeys.value = keys;
}

function toggleAll() {
  selectedKeys.value = allSelected.value ? new Set() : new Set(selectableLines.value.map((l) => l.key));
}

const clearSelection = () => (selectedKeys.value = new Set());

/** Lignes concernées par copier / couper / supprimer / glisser (jamais la ligne de saisie vide). */
const selectionBlock = () => selectedLines.value.filter((l) => l !== entryLine());

/** Copie la sélection ; le texte part aussi dans le presse-papiers système (Excel, mail…). */
function copySelection(e?: ClipboardEvent) {
  const block = selectionBlock();
  if (!block.length) return;
  const text = copy(block, pricingKey.value);
  if (e?.clipboardData) e.clipboardData.setData("text/plain", text);
  else navigator.clipboard?.writeText(text).catch(() => {});
  toast.add({ severity: "info", summary: `${block.length} ligne(s) copiée(s)`, life: 1500 });
}

function deleteSelection() {
  const block = new Set(selectionBlock());
  lines.value = lines.value.filter((l) => !block.has(l));
  if (!entryLine()) lines.value.push(blankLine());
  clearSelection();
}

function cutSelection(e?: ClipboardEvent) {
  copySelection(e);
  deleteSelection();
}

/** Colle après `anchor` ; à défaut après la dernière ligne sélectionnée, sinon en fin de devis. */
async function pasteLines(anchor: EditLine | null = null) {
  if (!clipboard.value) return;
  const pasted: EditLine[] = clipboard.value.lines.map((l) => ({
    ...blankLine(),
    ...withTexts(l),
    resolvedRef: l.kind === "item" ? l.product_ref : null,
  }));
  const selection = selectedLines.value;
  anchor ??= selection.length ? selection[selection.length - 1] : null;
  lines.value = insertBlock(lines.value, pasted, anchor, entryLine());
  const pastedKeys = new Set(pasted.map((l) => l.key));
  selectedKeys.value = pastedKeys;

  // Copiées depuis un devis aux conditions différentes : on applique celles de ce devis.
  if (hasClient.value && clipboard.value.pricingKey !== pricingKey.value) {
    // On passe par lines.value (objets réactifs) pour que l'écran se mette à jour.
    const items = lines.value.filter((l) => pastedKeys.has(l.key) && l.kind === "item");
    await Promise.all(items.map((l) => resolve(l)));
    if (items.length) toast.add({ severity: "info", summary: "Prix recalculés pour ce client", life: 2500 });
  }
}

// ---------- Raccourcis clavier ⌘C / ⌘X / ⌘V / Échap (Ctrl sous Windows) ----------
// Le copier-coller de texte dans les champs reste prioritaire :
// - ⌘C / ⌘X copient les lignes sélectionnées, sauf si du texte est sélectionné dans le champ actif ;
// - ⌘V colle les lignes si le presse-papiers contient encore nos lignes (sinon, collage de texte normal),
//   juste après la ligne où se trouve le curseur.

function isTextField(el: EventTarget | null): el is HTMLElement {
  if (!(el instanceof HTMLElement)) return false;
  if (el.isContentEditable || el instanceof HTMLTextAreaElement) return true;
  return el instanceof HTMLInputElement && !["checkbox", "radio", "button"].includes(el.type);
}

function hasTextSelection(el: EventTarget | null) {
  if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement) {
    try {
      return el.selectionStart !== el.selectionEnd;
    } catch {
      return false;
    }
  }
  return !!window.getSelection()?.toString();
}

const copiesLines = (target: EventTarget | null) => selectedKeys.value.size > 0 && !hasTextSelection(target);

/** Ligne du tableau contenant l'élément (pour coller « ici »). */
function lineAt(el: EventTarget | null): EditLine | null {
  const row = el instanceof Element ? el.closest<HTMLElement>("tr[data-key]") : null;
  const key = row ? Number(row.dataset.key) : NaN;
  return lines.value.find((l) => l.key === key) ?? null;
}

function onCopy(e: ClipboardEvent) {
  if (!copiesLines(e.target)) return;
  e.preventDefault();
  copySelection(e);
}

function onCut(e: ClipboardEvent) {
  if (!copiesLines(e.target)) return;
  e.preventDefault();
  cutSelection(e);
}

function onPaste(e: ClipboardEvent) {
  if (!clipboard.value) return;
  const target = document.activeElement;
  // Dans un champ, on ne colle des lignes que si le presse-papiers contient toujours nos lignes.
  if (isTextField(target) && !holdsLines(e.clipboardData?.getData("text/plain") ?? "")) return;
  e.preventDefault();
  pasteLines(lineAt(target));
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === "Escape" && selectedKeys.value.size && !isTextField(e.target)) {
    clearSelection();
    return;
  }
  if (!(e.metaKey || e.ctrlKey) || e.shiftKey || e.altKey) return;
  const key = e.key.toLowerCase();
  // Hors champ texte, WebKit n'émet pas toujours copy / cut / paste : on agit dès la touche.
  if ((key === "c" || key === "x") && copiesLines(e.target) && !isTextField(e.target)) {
    e.preventDefault();
    if (key === "c") copySelection();
    else cutSelection();
  } else if ((key === "c" || key === "x") && copiesLines(e.target)) {
    // Dans un champ sans texte sélectionné, « Copier » est inactif : on déclenche l'événement nous-mêmes.
    e.preventDefault();
    if (!document.execCommand(key === "c" ? "copy" : "cut")) {
      if (key === "c") copySelection();
      else cutSelection();
    }
  } else if (key === "v" && clipboard.value && !isTextField(e.target)) {
    e.preventDefault();
    pasteLines(lineAt(e.target));
  }
}

onMounted(() => {
  window.addEventListener("keydown", onKeydown);
  document.addEventListener("copy", onCopy);
  document.addEventListener("cut", onCut);
  document.addEventListener("paste", onPaste);
});
onBeforeUnmount(() => {
  window.removeEventListener("keydown", onKeydown);
  document.removeEventListener("copy", onCopy);
  document.removeEventListener("cut", onCut);
  document.removeEventListener("paste", onPaste);
});

function removeLine(line: EditLine) {
  lines.value = lines.value.filter((l) => l !== line);
  if (!lines.value.length) lines.value.push(blankLine());
}

/** Prix net de la ligne (remise supplémentaire comprise) sous le prix seuil. */
const isBelowThreshold = (l: EditLine) =>
  l.threshold_price != null && netUnitPrice(l.unit_price, l.discount) < l.threshold_price;

/** Champs enregistrés d'une ligne (sans l'état d'édition). */
const toQuoteLine = (l: EditLine): QuoteLine => ({
  kind: l.kind,
  product_ref: l.product_ref,
  enedis_code: l.enedis_code ?? null,
  designation: l.designation,
  quantity: l.quantity,
  unit_price: l.unit_price,
  discount: l.discount ?? 0,
  is_option: !!l.is_option,
  price_source: l.price_source,
  public_price: l.public_price,
  discounted_price: l.discounted_price ?? null,
  lpn_price: l.lpn_price ?? null,
  lpn_list: l.lpn_list ?? null,
  threshold_price: l.threshold_price,
});

function toQuote(): Quote {
  return {
    id: quoteId.value,
    number: number.value,
    client_code: clientCode.value,
    client_name: clientName.value.trim(),
    ...contact.value,
    ...pricing.value,
    date: date.value,
    notes: notes.value.trim() || null,
    total_ht: total.value,
    discount_pct: globalDiscount.value ?? 0,
    total_net: netTotal.value,
    total_options: totals.value.options,
    lines: [
      ...lines.value
        // Articles sans réf et lignes de texte vides : ignorés.
        .filter((l) => (l.kind === "item" ? l.product_ref.trim() : l.kind === "subtotal" || l.designation.trim()))
        .map((l) => ({
          ...toQuoteLine(l),
          designation: l.kind === "subtotal" ? l.designation.trim() || "Sous-total" : l.designation,
          quantity: l.quantity ?? 0,
          discount: l.discount ?? 0,
        })),
      // Frais toujours en dernier.
      ...feeLines.value.map((l) => ({
        ...toQuoteLine(l),
        designation: l.designation.trim() || FEE_LABELS[l.kind as FeeKind],
        quantity: 1,
      })),
    ],
  };
}

async function save(): Promise<Quote | null> {
  if (!hasClient.value) {
    toast.add({ severity: "warn", summary: "Choisissez un client", life: 2500 });
    return null;
  }
  if (!filledLines.value.length) {
    toast.add({ severity: "warn", summary: "Ajoutez au moins une ligne", life: 2500 });
    return null;
  }
  saving.value = true;
  try {
    const saved = await api.saveQuote(toQuote());
    const isNew = quoteId.value == null;
    quoteId.value = saved.id;
    number.value = saved.number;
    // Enregistré en base : plus besoin du brouillon.
    if (isNew) removeDraft("nouveau");
    markSaved();
    if (isNew) router.replace(`/devis/${saved.id}`);
    toast.add({ severity: "success", summary: `Devis ${saved.number} enregistré`, life: 2000 });
    return saved;
  } catch (e) {
    toast.add({ severity: "error", summary: "Enregistrement", detail: errorMessage(e) });
    return null;
  } finally {
    saving.value = false;
  }
}

// PDF + documentation technique : le devis est enregistré, puis la fenêtre de choix des documents s'ouvre.
const docsDialogVisible = ref(false);
const docsDialogQuote = ref<Quote | null>(null);

async function savePdfWithDocs() {
  const saved = await save();
  if (!saved) return;
  docsDialogQuote.value = saved;
  docsDialogVisible.value = true;
}

async function savePdf() {
  const saved = await save();
  if (!saved) return;
  try {
    const result = await exportQuotePdf(saved);
    if (result) {
      toast.add({ severity: "success", summary: "PDF enregistré", detail: result.path, life: 4000 });
      if (result.warning) toast.add({ severity: "warn", summary: "Conditions générales", detail: result.warning });
    }
  } catch (e) {
    toast.add({ severity: "error", summary: "Génération du PDF", detail: errorMessage(e) });
  }
}

/** Devis enregistrés avant l'ajout des colonnes prix public / seuil : on les complète (sans toucher au PU). */
async function fillMissingReferencePrices() {
  if (!hasClient.value) return;
  const ctx = pricing.value;
  const missing = lines.value.filter(
    (l) =>
      l.kind === "item" &&
      l.product_ref &&
      (l.public_price == null || (l.discounted_price == null && l.lpn_price == null)),
  );
  await Promise.all(
    missing.map(async (l) => {
      try {
        const p = await api.resolvePrice(ctx, l.product_ref);
        Object.assign(l, {
          public_price: p.public_price,
          threshold_price: p.threshold_price,
          discounted_price: p.discounted_price,
          lpn_price: p.lpn_price,
          lpn_list: p.lpn_list,
          enedis_code: l.enedis_code ?? p.enedis_code,
        });
      } catch {
        // Produit disparu du catalogue : colonnes laissées vides.
      }
    }),
  );
}

// ---------- Brouillon local : le devis en cours est gardé à chaque modification ----------

const draftKey = () => (quoteId.value == null ? "nouveau" : String(quoteId.value));
/** Faux pendant un chargement : on n'écrit pas de brouillon d'un état à moitié chargé. */
const draftReady = ref(false);
/** État tel qu'en base (ou vierge) : tant qu'on y est, pas de brouillon. */
let savedSnapshot = "";
const hasDraft = ref(false);

/** Ce qui compte pour savoir si le devis a changé (pas le prix public / seuil complétés en tâche de fond). */
const snapshot = () =>
  JSON.stringify({
    client: [clientCode.value, clientName.value, pricingKey.value, contact.value],
    date: date.value,
    notes: notes.value,
    discount: globalDiscount.value,
    lines: [...lines.value.filter((l) => !isEntryLine(l)), ...feeLines.value].map((l) => [
      l.kind,
      l.product_ref,
      l.designation,
      l.quantity,
      l.unit_price,
      l.discount,
      l.is_option,
      l.price_source,
    ]),
  });

function markSaved() {
  savedSnapshot = snapshot();
  removeDraft(draftKey());
  hasDraft.value = false;
}

function persistDraft() {
  if (!draftReady.value) return;
  if (snapshot() === savedSnapshot) {
    removeDraft(draftKey());
    hasDraft.value = false;
    return;
  }
  const draft: QuoteDraft = {
    number: number.value,
    client_code: clientCode.value,
    client_name: clientName.value,
    ...contact.value,
    ...pricing.value,
    date: date.value,
    notes: notes.value,
    discount_pct: globalDiscount.value,
    lines: [...lines.value.filter((l) => !isEntryLine(l)), ...feeLines.value].map(toQuoteLine),
    savedAt: new Date().toISOString(),
  };
  writeDraft(draftKey(), draft);
  hasDraft.value = true;
}

let draftTimer: ReturnType<typeof setTimeout> | undefined;
watch([clientCode, clientName, pricingKey, contact, date, notes, globalDiscount, lines], () => {
  clearTimeout(draftTimer);
  draftTimer = setTimeout(persistDraft, 300);
}, { deep: true });
onBeforeUnmount(() => {
  clearTimeout(draftTimer);
  persistDraft();
});
window.addEventListener("beforeunload", persistDraft);
onBeforeUnmount(() => window.removeEventListener("beforeunload", persistDraft));

function applyDraft(draft: QuoteDraft) {
  date.value = draft.date;
  notes.value = draft.notes;
  globalDiscount.value = draft.discount_pct ?? 0;
  const known = draft.client_code ? clients.value.find((c) => c.code === draft.client_code) : undefined;
  setClientFields({
    client_code: draft.client_code,
    client_name: draft.client_name,
    // Brouillon d'avant les conditions propres au devis : on reprend celles du client.
    discount_cfa: draft.discount_cfa ?? known?.discount_cfa ?? 0,
    discount_cfo: draft.discount_cfo ?? known?.discount_cfo ?? 0,
    price_lists: draft.price_lists ?? known?.price_lists ?? [],
    forced_price_list: draft.forced_price_list ?? null,
    contact_name: draft.contact_name,
    contact_email: draft.contact_email,
    contact_phone: draft.contact_phone,
    sales_rep: draft.sales_rep ?? known?.sales_rep ?? "",
  });
  const restored = draft.lines.map((l) => ({
    ...blankLine(),
    ...withTexts(l),
    resolvedRef: l.kind === "item" ? l.product_ref : null,
  }));
  lines.value = [...restored.filter((l) => !isFee(l.kind)), blankLine()];
  feeLines.value = restored.filter((l) => isFee(l.kind));
  hasDraft.value = true;
}

/** Vide le devis en cours (et son brouillon) ; pour un devis enregistré, ouvre un nouveau devis vierge. */
function startOver() {
  confirm.require({
    header: "Repartir de zéro",
    message: quoteId.value
      ? "Abandonner les modifications non enregistrées et commencer un nouveau devis vierge ? Le devis enregistré n'est pas modifié."
      : "Vider le devis en cours (client, lignes, notes) ?",
    icon: "pi pi-refresh",
    rejectProps: { label: "Annuler", severity: "secondary", text: true },
    acceptProps: { label: "Repartir de zéro", severity: "danger" },
    accept: () => {
      clearTimeout(draftTimer);
      draftReady.value = false;
      removeDraft(draftKey());
      removeDraft("nouveau");
      if (quoteId.value) router.push("/devis/nouveau");
      else load(undefined);
    },
  });
}

/** Client et conditions du devis ; la recherche affiche le client de la base s'il existe encore. */
function setClientFields(q: Pick<Quote, "client_code" | "client_name"> & PricingContext & Partial<QuoteContact>) {
  contact.value = {
    contact_name: q.contact_name ?? "",
    contact_email: q.contact_email ?? "",
    contact_phone: q.contact_phone ?? "",
    sales_rep: q.sales_rep ?? "",
  };
  clientCode.value = q.client_code;
  clientName.value = q.client_name;
  discountCfa.value = q.discount_cfa;
  discountCfo.value = q.discount_cfo;
  priceLists.value = [...q.price_lists];
  forcedList.value = q.forced_price_list ?? null;
  client.value = q.client_code
    ? (clients.value.find((c) => c.code === q.client_code) ??
      // Client disparu depuis un réimport : on affiche ce que le devis a mémorisé.
      ({ code: q.client_code, name: q.client_name, price_lists: [] } as unknown as Client))
    : null;
}

async function load(id: string | undefined) {
  dismissedFees.clear();
  clearSelection();
  clearTimeout(draftTimer);
  draftReady.value = false;
  try {
    await loadSaved(id);
    savedSnapshot = snapshot();
    hasDraft.value = false;
    // Modifications non enregistrées laissées la dernière fois : on les reprend.
    let draft = readDraft(draftKey());
    // Brouillon d'un devis supprimé dont l'identifiant a été réutilisé : on l'écarte.
    if (draft && draft.number !== undefined && (draft.number ?? null) !== number.value) {
      removeDraft(draftKey());
      draft = null;
    }
    if (draft) {
      applyDraft(draft);
      toast.add({
        severity: "info",
        summary: "Brouillon restauré",
        detail: "Les modifications non enregistrées ont été reprises.",
        life: 3000,
      });
    }
  } finally {
    await nextTick();
    draftReady.value = true;
  }
}

async function loadSaved(id: string | undefined) {
  try {
    if (!id) {
      // Nouveau devis : formulaire vierge.
      quoteId.value = null;
      number.value = null;
      date.value = todayIso();
      notes.value = "";
      globalDiscount.value = 0;
      setClientFields({
        client_code: null,
        client_name: "",
        discount_cfa: 0,
        discount_cfo: 0,
        price_lists: [],
        forced_price_list: null,
      });
      lines.value = [blankLine()];
      feeLines.value = [];
      return;
    }
    const q = await api.getQuote(Number(id));
    quoteId.value = q.id;
    number.value = q.number;
    date.value = q.date;
    notes.value = q.notes ?? "";
    globalDiscount.value = q.discount_pct;
    setClientFields(q);
    const loaded: EditLine[] = q.lines.map((l) => ({
      ...withTexts({
        ...l,
        // Une quantité enregistrée à 0 revient vide, comme à la saisie.
        quantity: l.kind === "item" && !l.quantity ? null : l.quantity,
        unit_price: round2(l.unit_price),
      }),
      key: nextKey++,
      resolvedRef: l.product_ref,
      pendingRef: null,
      error: null,
    }));
    lines.value = [...loaded.filter((l) => !isFee(l.kind)), blankLine()];
    feeLines.value = loaded.filter((l) => isFee(l.kind));
    fillMissingReferencePrices();
  } catch (e) {
    toast.add({ severity: "error", summary: "Chargement du devis", detail: errorMessage(e) });
  }
}

// Le composant est réutilisé entre /devis/nouveau et /devis/:id (bouton « Nouveau devis », duplication).
// Après la création d'un devis, l'URL passe à /devis/:id sans recharger : quoteId est déjà à jour.
watch(
  () => props.id,
  (id) => {
    if (id !== (quoteId.value == null ? undefined : String(quoteId.value))) load(id);
  },
);

onMounted(async () => {
  try {
    const [list, priceListRows, settings] = await Promise.all([
      api.listClients(),
      api.listPriceLists(),
      api.getSettings(),
    ]);
    clients.value = list;
    allPriceLists.value = priceListRows;
    favorites.value = favoriteLists(settings);
    feeRules.value = feeRulesFrom(settings);
  } catch (e) {
    toast.add({ severity: "error", summary: "Chargement des clients", detail: errorMessage(e) });
  }
  await load(props.id);
});
</script>

<template>
  <div class="page">
    <div class="page-header">
      <Button icon="pi pi-arrow-left" text rounded @click="router.push('/devis')" />
      <h1>{{ number ? `Devis ${number}` : "Nouveau devis" }}</h1>
      <span
        v-if="hasDraft"
        v-tooltip.bottom="'Gardé sur cet ordinateur ; restauré si vous revenez sur ce devis'"
        class="muted draft-badge"
      >
        <i class="pi pi-circle-fill" /> Modifications non enregistrées
      </span>
      <span class="spacer" />
      <Button label="Repartir de zéro" icon="pi pi-refresh" severity="secondary" text @click="startOver" />
      <Button label="Enregistrer" icon="pi pi-save" severity="secondary" :loading="saving" @click="save" />
      <Button label="Générer PDF" icon="pi pi-file-pdf" :loading="saving" @click="savePdf" />
      <Button
        v-tooltip.bottom="'Devis suivi de la documentation technique des produits'"
        label="Devis PDF + Docs"
        icon="pi pi-book"
        :loading="saving"
        @click="savePdfWithDocs"
      />
    </div>
    <QuoteDocsDialog v-model:visible="docsDialogVisible" :quote="docsDialogQuote" />

    <div class="card head">
      <div class="field client-field">
        <label for="client">Client</label>
        <div class="client-picker">
          <AutoComplete
            v-model="client"
            input-id="client"
            :suggestions="clientSuggestions"
            option-label="name"
            placeholder="Rechercher un client (nom ou code)…"
            force-selection
            fluid
            @complete="searchClients"
            @option-select="(e: { value: Client }) => applyClient(e.value)"
            @keydown.tab="onClientTab"
          >
            <template #option="{ option }">
              <div class="opt">
                <span>{{ option.name }}</span>
                <span class="muted mono">{{ option.code }}</span>
              </div>
            </template>
            <template #empty>
              <span class="muted">Aucun client trouvé — « Client ponctuel » pour un client hors base</span>
            </template>
          </AutoComplete>
          <Button
            v-tooltip.bottom="'Client qui n\'existe que dans ce devis'"
            label="Client ponctuel"
            icon="pi pi-user-plus"
            severity="secondary"
            outlined
            @click="openEphemeral()"
          />
        </div>

        <!-- Conditions du devis : modifiables ici sans toucher à la fiche client. -->
        <div v-if="hasClient" class="conditions">
          <InputText
            v-model="clientName"
            v-tooltip.bottom="'Raison sociale imprimée sur le devis'"
            size="small"
            class="conditions-name"
          />
          <Tag v-if="clientCode" :value="`Client ${clientCode}`" severity="secondary" />
          <Tag v-else value="Client ponctuel" severity="info" />
          <label for="cfa">CFA</label>
          <InputNumber
            v-model="discountCfa"
            input-id="cfa"
            :min="0"
            :max="100"
            :max-fraction-digits="2"
            suffix=" %"
            size="small"
            input-class="pct"
            @update:model-value="repriceLines(true)"
          />
          <label for="cfo">CFO</label>
          <InputNumber
            v-model="discountCfo"
            input-id="cfo"
            :min="0"
            :max="100"
            :max-fraction-digits="2"
            suffix=" %"
            size="small"
            input-class="pct"
            @update:model-value="repriceLines(true)"
          />
          <template v-if="clientCode">
            <Tag v-for="l in priceLists" :key="l" :value="l" severity="secondary" />
            <span v-if="!priceLists.length" class="muted">aucune liste de prix</span>
          </template>
          <Select
            v-else
            v-model="ephemeralList"
            :options="priceListOptions"
            option-label="label"
            option-value="code"
            filter
            show-clear
            placeholder="Aucune liste de prix"
            size="small"
            class="list-select"
            :virtual-scroller-options="{ itemSize: 36 }"
          />
          <label for="forced-list">Liste forcée</label>
          <Select
            v-model="forcedList"
            input-id="forced-list"
            :options="forcedOptions"
            option-label="label"
            option-value="code"
            show-clear
            :placeholder="forcedOptions.length ? 'Aucune' : 'Aucune favorite (Réglages)'"
            :disabled="!forcedOptions.length"
            size="small"
            class="list-select"
            @change="onForcedListChange"
          />
          <span class="muted hint">pour ce devis uniquement</span>
        </div>
        <!-- Contact chez le client et commercial : imprimés sur le PDF. -->
        <div v-if="hasClient" class="conditions contact-row">
          <label for="contact-name">Contact</label>
          <InputText id="contact-name" v-model="contact.contact_name" placeholder="Nom du contact" size="small" />
          <InputText v-model="contact.contact_email" placeholder="Email" size="small" type="email" class="contact-email" />
          <InputText v-model="contact.contact_phone" placeholder="Téléphone" size="small" class="contact-phone" />
          <label for="sales-rep">Commercial</label>
          <InputText id="sales-rep" v-model="contact.sales_rep" placeholder="Commercial" size="small" />
        </div>

        <Dialog v-model:visible="ephemeralVisible" modal header="Client ponctuel" :style="{ width: '480px' }">
          <p class="muted dialog-hint">Ce client n'est pas ajouté à la base : il n'existe que dans ce devis.</p>
          <form class="form-grid" @submit.prevent="applyEphemeral">
            <label for="eph-name">Raison sociale *</label>
            <InputText id="eph-name" v-model="ephemeral.name" autofocus />
            <label for="eph-cfa">Remise CFA</label>
            <InputNumber v-model="ephemeral.cfa" input-id="eph-cfa" :min="0" :max="100" :max-fraction-digits="2" suffix=" %" />
            <label for="eph-cfo">Remise CFO</label>
            <InputNumber v-model="ephemeral.cfo" input-id="eph-cfo" :min="0" :max="100" :max-fraction-digits="2" suffix=" %" />
            <label>Liste de prix</label>
            <Select
              v-model="ephemeral.list"
              :options="priceListOptions"
              option-label="label"
              option-value="code"
              filter
              show-clear
              placeholder="Aucune (prix public − remises)"
              :virtual-scroller-options="{ itemSize: 36 }"
            />
            <button type="submit" hidden />
          </form>
          <template #footer>
            <Button label="Annuler" severity="secondary" text @click="ephemeralVisible = false" />
            <Button label="Valider" icon="pi pi-check" @click="applyEphemeral" />
          </template>
        </Dialog>
      </div>
      <div class="field">
        <label for="date">Date</label>
        <InputText id="date" v-model="date" type="date" />
      </div>
    </div>

    <div class="card">
      <div v-if="selectedKeys.size || clipboard" class="selection-bar">
        <template v-if="selectedKeys.size">
          <strong>{{ selectedKeys.size }} sélectionnée(s)</strong>
          <Button v-tooltip.bottom="`${MOD}C`" label="Copier" icon="pi pi-copy" text size="small" @click="copySelection()" />
          <Button v-tooltip.bottom="`${MOD}X`" label="Couper" icon="pi pi-clone" text size="small" @click="cutSelection()" />
          <Button label="Supprimer" icon="pi pi-trash" text size="small" severity="danger" @click="deleteSelection" />
          <Button v-tooltip.bottom="'Échap'" icon="pi pi-times" text rounded size="small" severity="secondary" @click="clearSelection" />
        </template>
        <span class="spacer" />
        <Button
          v-if="clipboard"
          v-tooltip.bottom="`${selectedKeys.size ? 'Colle après la dernière ligne sélectionnée' : 'Colle en fin de devis'} (${MOD}V)`"
          :label="`Coller ${clipboard.lines.length} ligne(s)`"
          icon="pi pi-clipboard"
          size="small"
          severity="secondary"
          @click="pasteLines()"
        />
      </div>
      <table class="lines">
        <thead>
          <tr>
            <th style="width: 34px"></th>
            <th style="width: 28px" class="select-cell">
              <input
                type="checkbox"
                :checked="allSelected"
                :disabled="!selectableLines.length"
                aria-label="Tout sélectionner"
                @click="toggleAll"
              />
            </th>
            <th style="width: 90px">ENEDIS</th>
            <th style="width: 190px">Référence</th>
            <th>Désignation</th>
            <th style="width: 90px" class="num">Qté</th>
            <th style="width: 95px" class="num">Prix public</th>
            <th v-tooltip.top="'Prix public − remise CFA / CFO du devis'" style="width: 95px" class="num">Prix remisé</th>
            <th v-tooltip.top="'Prix négocié de la liste de prix'" style="width: 95px" class="num">LPN</th>
            <th style="width: 120px" class="num">PU HT (€)</th>
            <th v-tooltip.top="'Remise supplémentaire sur le prix de la ligne'" style="width: 90px" class="num">
              Remise sup. (%)
            </th>
            <th style="width: 110px" class="num">Total HT</th>
            <th v-tooltip.top="'Ligne en option : hors total HT, comptée dans « Total options »'" style="width: 54px" class="select-cell">
              Option
            </th>
            <th v-tooltip.top="'Non imprimé sur le devis'" style="width: 100px" class="num seuil">Prix seuil</th>
            <th style="width: 44px"></th>
          </tr>
        </thead>
        <tbody ref="tbodyEl">
          <template v-for="line in lines" :key="line.key">
            <tr
              v-if="line.kind !== 'item'"
              :data-key="line.key"
              :class="[
                `${line.kind}-row`,
                {
                  selected: selectedKeys.has(line.key),
                  dragging: drag?.moved && drag.keys.has(line.key),
                  'drop-before': dropKey === line.key,
                },
              ]"
            >
              <td
                v-tooltip.left="'Glisser pour déplacer'"
                class="drag-cell drag-handle"
                @pointerdown="startDrag(line, $event)"
              >
                <i class="pi pi-bars" />
              </td>
              <td class="select-cell">
                <input
                  type="checkbox"
                  :checked="selectedKeys.has(line.key)"
                  aria-label="Sélectionner la ligne"
                  @click="onSelectClick(line, $event)"
                />
              </td>
              <template v-if="line.kind === 'text'">
                <td colspan="11">
                  <Textarea
                    :id="`text-${line.key}`"
                    v-model="line.designation"
                    rows="1"
                    auto-resize
                    fluid
                    size="small"
                    placeholder="Texte libre…"
                    class="text-line"
                  />
                </td>
                <td></td>
              </template>
              <!-- Titre : sépare le devis en paragraphes (gras, rouge, plus grand). -->
              <template v-else-if="line.kind === 'title'">
                <td colspan="11">
                  <InputText
                    :id="`text-${line.key}`"
                    v-model="line.designation"
                    fluid
                    size="small"
                    placeholder="Titre…"
                    class="title-line"
                  />
                </td>
                <td></td>
              </template>
              <template v-else>
                <td colspan="9">
                  <InputText v-model="line.designation" fluid size="small" class="subtotal-label" />
                </td>
                <td class="num subtotal-amount">{{ formatEuro(subtotalAmounts.get(line.key)) }}</td>
                <td></td>
                <td></td>
              </template>
              <td>
                <Button
                  icon="pi pi-times"
                  text
                  rounded
                  size="small"
                  severity="secondary"
                  @click="removeLine(line)"
                />
              </td>
            </tr>
            <tr
              v-else
              :data-key="line.key"
              :class="{
                selected: selectedKeys.has(line.key),
                'option-row': line.is_option,
                dragging: drag?.moved && drag.keys.has(line.key),
                'drop-before': dropKey === line.key,
              }"
            >
              <td
                v-if="!isEntryLine(line)"
                v-tooltip.left="'Glisser pour déplacer'"
                class="drag-cell drag-handle"
                @pointerdown="startDrag(line, $event)"
              >
                <i class="pi pi-bars" />
              </td>
              <td v-else class="drag-cell"></td>
              <td class="select-cell">
                <input
                  v-if="!isEntryLine(line)"
                  type="checkbox"
                  :checked="selectedKeys.has(line.key)"
                  aria-label="Sélectionner la ligne"
                  @click="onSelectClick(line, $event)"
                />
              </td>
              <td>
                <InputText
                  :id="`enedis-${line.key}`"
                  :model-value="line.enedis_code ?? ''"
                  autocomplete="off"
                  placeholder="Code…"
                  fluid
                  size="small"
                  class="mono enedis-input"
                  @update:model-value="(v: string | undefined) => (line.enedis_code = v ?? '')"
                  @keydown.enter="commitEnedis(line, true)"
                  @blur="commitEnedis(line, false)"
                />
              </td>
              <td>
                <AutoComplete
                  :model-value="line.product_ref"
                  :input-id="`ref-${line.key}`"
                  :suggestions="productSuggestions"
                  option-label="ref"
                  :delay="200"
                  :min-length="2"
                  :show-empty-message="false"
                  fluid
                  size="small"
                  placeholder="Réf…"
                  :invalid="!!line.error"
                  @update:model-value="(v: string | ProductHit) => (line.product_ref = typeof v === 'string' ? v : v.ref)"
                  @complete="searchProducts($event, line)"
                  @option-select="commitRef(line)"
                  @blur="onRefBlur(line)"
                  @keydown.enter="commitRef(line)"
                >
                  <template #option="{ option }">
                    <div class="opt">
                      <span class="mono">{{ option.ref }}</span>
                      <span v-if="option.enedis_code" class="mono muted">{{ option.enedis_code }}</span>
                      <span class="muted">{{ option.designation }}</span>
                    </div>
                  </template>
                </AutoComplete>
              </td>
              <td>
                <InputText v-model="line.designation" fluid size="small" />
                <small v-if="line.error" class="warn">{{ line.error }}</small>
              </td>
              <td>
                <InputText
                  :id="`qty-${line.key}`"
                  :model-value="line.qtyText"
                  inputmode="decimal"
                  autocomplete="off"
                  fluid
                  size="small"
                  class="num"
                  @update:model-value="(v: string | undefined) => onQtyInput(line, v ?? '')"
                  @blur="line.qtyText = editText(line.quantity, 3)"
                  @keydown.enter="moveToQuantity(line, 1, $event)"
                  @keydown.down="moveToQuantity(line, 1, $event)"
                  @keydown.up="moveToQuantity(line, -1, $event)"
                />
              </td>
              <td class="num muted">{{ formatUnitPrice(line.public_price) }}</td>
              <!-- En gras : le prix qui a fixé le PU HT (le plus bas des deux, ou la liste forcée). -->
              <td class="num ref-price" :class="{ used: line.price_source?.startsWith('Public') }">
                {{ formatUnitPrice(line.discounted_price) }}
              </td>
              <td
                v-tooltip.top="line.lpn_list ? `Liste ${line.lpn_list}` : 'Aucun prix négocié'"
                class="num ref-price"
                :class="{ used: line.lpn_price != null && line.price_source === line.lpn_list }"
              >
                {{ formatUnitPrice(line.lpn_price) }}
              </td>
              <!-- Source du prix en infobulle (sur une 2e ligne, elle doublait la hauteur des lignes). -->
              <td
                v-tooltip.top="
                  line.price_source
                    ? `Prix : ${line.price_source}${isBelowThreshold(line) ? ' · sous le prix seuil' : ''}`
                    : ''
                "
              >
                <InputText
                  :model-value="line.priceText"
                  inputmode="decimal"
                  autocomplete="off"
                  fluid
                  size="small"
                  class="num"
                  :invalid="isBelowThreshold(line)"
                  @update:model-value="(v: string | undefined) => onPriceInput(line, v ?? '')"
                  @blur="onPriceBlur(line)"
                />
              </td>
              <td>
                <InputText
                  :model-value="line.discountText"
                  inputmode="decimal"
                  autocomplete="off"
                  placeholder="–"
                  fluid
                  size="small"
                  class="num"
                  @update:model-value="(v: string | undefined) => onDiscountInput(line, v ?? '')"
                  @blur="line.discountText = line.discount ? editText(line.discount, 3) : ''"
                />
              </td>
              <td class="num">{{ line.product_ref && line.quantity != null ? formatEuro(lineTotal(line)) : "" }}</td>
              <td class="select-cell">
                <input
                  v-if="!isEntryLine(line)"
                  v-model="line.is_option"
                  type="checkbox"
                  aria-label="Ligne en option"
                />
              </td>
              <td class="num seuil" :class="{ warn: isBelowThreshold(line) }">
                {{ formatUnitPrice(line.threshold_price) }}
              </td>
              <td>
                <Button
                  v-if="line.product_ref"
                  icon="pi pi-times"
                  text
                  rounded
                  size="small"
                  severity="secondary"
                  @click="removeLine(line)"
                />
              </td>
            </tr>
          </template>
        </tbody>
        <tfoot>
          <!-- Ajout de lignes : texte, sous-total, frais de port, frais de facturation. -->
          <tr>
            <td colspan="2"></td>
            <td colspan="13">
              <div class="add-lines">
                <Button
                  v-tooltip.bottom="'Titre de paragraphe, ajouté en fin de devis, à déplacer avec la poignée'"
                  label="Titre"
                  icon="pi pi-bars"
                  text
                  size="small"
                  @click="insertSpecialLine('title')"
                />
                <Button
                  v-tooltip.bottom="'Ajoutée en fin de devis, à déplacer avec la poignée'"
                  label="Texte"
                  icon="pi pi-align-left"
                  text
                  size="small"
                  @click="insertSpecialLine('text')"
                />
                <Button
                  v-tooltip.bottom="'Additionne les articles depuis le sous-total précédent'"
                  label="Sous-total"
                  icon="pi pi-calculator"
                  text
                  size="small"
                  @click="insertSpecialLine('subtotal')"
                />
                <Button
                  v-tooltip.bottom="'Toujours en bas du devis, non remisés'"
                  label="Frais de port"
                  icon="pi pi-truck"
                  text
                  size="small"
                  :disabled="hasFee('shipping')"
                  @click="addFee('shipping')"
                />
                <Button
                  v-tooltip.bottom="'Toujours en bas du devis, non remisés'"
                  label="Frais de facturation"
                  icon="pi pi-receipt"
                  text
                  size="small"
                  :disabled="hasFee('billing')"
                  @click="addFee('billing')"
                />
              </div>
            </td>
          </tr>
          <!-- Totaux dans l'ordre du calcul : produits, remise (produits seulement), frais, total. -->
          <tr>
            <td colspan="7"></td>
            <td colspan="4" class="num total-label">Total produits HT</td>
            <td class="num total">{{ formatEuro(totals.products) }}</td>
            <td colspan="3"></td>
          </tr>
          <tr class="global-discount">
            <td colspan="7"></td>
            <td colspan="3" class="num total-label">
              <label v-tooltip.top="'Les frais de port et de facturation ne sont pas remisés'" for="global-discount">Remise sur les produits</label>
            </td>
            <td>
              <InputNumber
                v-model="globalDiscount"
                input-id="global-discount"
                :min="0"
                :max="100"
                :max-fraction-digits="2"
                suffix=" %"
                size="small"
                fluid
                input-class="num"
              />
            </td>
            <td class="num">{{ globalDiscount ? `− ${formatEuro(totals.discount)}` : "" }}</td>
            <td colspan="3"></td>
          </tr>
          <tr v-if="globalDiscount">
            <td colspan="7"></td>
            <td colspan="4" class="num total-label">Total produits remisé HT</td>
            <td class="num total">{{ formatEuro(round2(totals.products - totals.discount)) }}</td>
            <td colspan="3"></td>
          </tr>
          <!-- Frais de port / de facturation : après la remise (jamais remisés), montant HT dans la colonne Total. -->
          <tr v-for="fee in feeLines" :key="fee.key" class="fee-row">
            <td colspan="7"></td>
            <td colspan="4">
              <InputText v-model="fee.designation" fluid size="small" class="fee-label" />
            </td>
            <td>
              <InputText
                :id="`fee-${fee.key}`"
                :model-value="fee.priceText"
                inputmode="decimal"
                autocomplete="off"
                placeholder="0,00"
                fluid
                size="small"
                class="num"
                @update:model-value="(v: string | undefined) => { fee.auto = false; onPriceInput(fee, v ?? ''); }"
                @blur="onPriceBlur(fee)"
              />
            </td>
            <td colspan="2" class="muted fee-hint">non remisé</td>
            <td>
              <Button
                v-tooltip.left="`Retirer les ${fee.designation.toLowerCase() || 'frais'}`"
                icon="pi pi-times"
                text
                rounded
                size="small"
                severity="secondary"
                @click="removeFee(fee)"
              />
            </td>
          </tr>
          <tr class="grand-total">
            <td colspan="7"></td>
            <td colspan="4" class="num total-label">Total HT</td>
            <td class="num total">{{ formatEuro(netTotal) }}</td>
            <td colspan="3"></td>
          </tr>
          <!-- Lignes en option : à part, tout en bas, hors total HT. -->
          <tr v-if="lines.some((l) => l.is_option && l.product_ref)" class="options-total">
            <td colspan="7"></td>
            <td colspan="4" class="num total-label">Total options HT</td>
            <td class="num total">{{ formatEuro(totals.options) }}</td>
            <td colspan="3"></td>
          </tr>
        </tfoot>
      </table>
    </div>

    <div class="card">
      <label for="notes" class="muted">Notes (imprimées sur le devis)</label>
      <Textarea id="notes" v-model="notes" rows="3" fluid auto-resize />
    </div>
  </div>
</template>

<style scoped>
.draft-badge {
  font-size: 0.9em;
}

.draft-badge i {
  font-size: 0.5rem;
  color: var(--app-warn);
  vertical-align: middle;
}

.head {
  display: flex;
  gap: 1.5rem;
  align-items: flex-start;
}

.field {
  display: flex;
  flex-direction: column;
  gap: 0.35rem;
}

.field label {
  color: var(--app-muted);
}

.client-field {
  flex: 1;
}

.client-picker {
  display: flex;
  gap: 0.5rem;
}

.client-picker > :first-child {
  flex: 1;
}

.conditions {
  display: flex;
  gap: 0.5rem;
  align-items: center;
  flex-wrap: wrap;
}

.conditions label {
  color: var(--app-muted);
  margin-left: 0.5rem;
}

.contact-row label:first-child {
  margin-left: 0;
}

.contact-email {
  width: 220px;
}

.contact-phone {
  width: 140px;
}

.conditions-name {
  width: 260px;
  font-weight: 600;
}

.conditions :deep(input.pct) {
  width: 80px;
  text-align: right;
}

.list-select {
  min-width: 220px;
}

.hint {
  font-size: 0.85em;
}

.dialog-hint {
  margin-top: 0;
}

.opt {
  display: flex;
  gap: 0.75rem;
  justify-content: space-between;
  width: 100%;
}

.lines {
  width: 100%;
  border-collapse: separate;
  border-spacing: 0;
}

.lines th {
  text-align: left;
  font-weight: 600;
  color: var(--app-muted);
  padding: 0 6px 8px;
  border-bottom: 1px solid var(--app-border);
}

.lines th.num {
  text-align: right;
}

/* Lignes serrées : une vingtaine d'articles visibles d'un coup dans la fenêtre. */
.lines td {
  padding: 1px 4px;
  vertical-align: top;
  border-bottom: 1px solid var(--app-border);
  font-size: 0.8125rem;
}

.lines td.num {
  padding-top: 5px;
}

.lines tbody :deep(.p-inputtext) {
  padding: 2px 6px;
  font-size: 0.8125rem;
}

.lines tbody :deep(.p-button.p-button-icon-only) {
  width: 1.6rem;
  height: 1.6rem;
}

.drag-cell {
  text-align: center;
}

.lines td.drag-cell {
  padding-top: 5px;
}

/* Toute la cellule sert de poignée (l'icône seule était trop petite à attraper). */
.drag-handle {
  cursor: grab;
  user-select: none;
}

.drag-handle i {
  color: var(--app-muted);
  opacity: 0.5;
}

.drag-handle:hover {
  background: color-mix(in srgb, var(--app-accent) 10%, transparent);
}

tr:hover .drag-handle i {
  opacity: 1;
}

/* Glisser-déposer : lignes déplacées estompées, trait vert à l'endroit du dépôt. */
/* Lignes en option : hors total, en italique avec un liseré. */
.lines :deep(.enedis-input) {
  font-size: 0.9em;
}

/* Prix remisé / LPN : en gris, sauf celui qui a fixé le PU HT. */
.lines td.ref-price {
  color: var(--app-muted);
}

.lines td.ref-price.used {
  color: var(--app-text);
  font-weight: 600;
}

.lines tr.option-row td {
  font-style: italic;
  background: color-mix(in srgb, var(--app-muted) 7%, transparent);
}

.lines tr.option-row td:first-child {
  box-shadow: inset 3px 0 0 var(--app-muted);
}

.options-total td {
  padding-top: 0.75rem;
}

.lines tr.dragging td {
  opacity: 0.35;
}

.lines tr.drop-before td {
  box-shadow: inset 0 3px 0 var(--app-accent);
}

:global(body.dragging-lines),
:global(body.dragging-lines *) {
  cursor: grabbing !important;
  user-select: none;
}

/* Texte : noir, gras (taille 10–11 sur le PDF). */
.text-row :deep(.text-line) {
  font-weight: 600;
  color: var(--app-text);
  border-color: transparent;
  background: transparent;
  box-shadow: none;
}

.text-row :deep(.text-line:hover),
.text-row :deep(.text-line:focus) {
  border-color: var(--app-border);
}

/* Titre : gras, rouge, plus grand (taille 12 sur le PDF). */
.lines .title-row :deep(.title-line) {
  font-weight: 700;
  font-size: 1.1rem;
  color: var(--app-accent);
  border-color: transparent;
  background: transparent;
  box-shadow: none;
}

.title-row :deep(.title-line:hover),
.title-row :deep(.title-line:focus) {
  border-color: var(--app-border);
}

.title-row td {
  padding-top: 0.9rem;
}

.subtotal-row td {
  background: color-mix(in srgb, var(--app-accent) 9%, transparent);
  border-bottom: 2px solid color-mix(in srgb, var(--app-accent) 40%, transparent);
}

.subtotal-row :deep(.subtotal-label) {
  font-weight: 600;
  text-align: right;
  background: transparent;
}

.subtotal-amount {
  font-weight: 700;
  font-size: 1rem;
}

.fee-row td:not(:first-child) {
  background: color-mix(in srgb, var(--app-muted) 6%, transparent);
}

.fee-row :deep(.fee-label) {
  text-align: right;
  font-weight: 600;
  background: transparent;
}

.lines td.fee-hint {
  padding-top: 5px;
  font-size: 0.85em;
}

.add-lines {
  display: flex;
  gap: 0.25rem;
  white-space: nowrap;
}

.seuil {
  color: var(--app-muted);
  font-size: 0.92em;
}

th.seuil {
  font-style: italic;
}

.selection-bar {
  display: flex;
  align-items: center;
  gap: 0.25rem;
  margin: -0.25rem 0 0.75rem;
  padding: 0.25rem 0.5rem;
  border-radius: 8px;
  background: color-mix(in srgb, var(--app-accent) 8%, transparent);
}

.selection-bar strong {
  margin-right: 0.5rem;
}

.selection-bar .spacer {
  flex: 1;
}

.select-cell {
  text-align: center;
}

.lines td.select-cell {
  padding-top: 3px;
}

.select-cell input {
  margin: 0;
}

.select-cell input {
  accent-color: var(--app-accent);
  cursor: pointer;
}

.lines tr.selected td {
  background: color-mix(in srgb, var(--app-accent) 10%, transparent);
}

.lines small {
  display: block;
  margin-top: 3px;
}

:deep(input.num) {
  text-align: right;
}

.total-label {
  font-weight: 600;
}

.lines tfoot td {
  border-bottom: none;
  font-size: 1.1rem;
}

.total {
  font-weight: 700;
}

/* Total HT final (remise et frais compris) : en rouge, filet au-dessus. */
.lines tfoot tr.grand-total td.total-label,
.lines tfoot tr.grand-total td.total {
  border-top: 2px solid var(--app-accent);
  color: var(--app-accent);
}
</style>
