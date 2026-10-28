<script setup lang="ts">
// Une ligne du tableau de l'éditeur de devis. Dans son propre composant pour qu'une frappe ne
// redessine que sa ligne (et non tout le tableau, lent au-delà de 200 lignes). Les styles du
// tableau restent dans QuoteEditView (règles en :deep).
import { inject } from "vue";
import AutoComplete from "primevue/autocomplete";
import InputText from "primevue/inputtext";
import Textarea from "primevue/textarea";
import Button from "primevue/button";
import type { ProductHit } from "../api";
import { formatEuro, formatUnitPrice } from "../format";
import { lineTotal } from "../quoteLines";
import { editText, isBelowThreshold, isCustom, LINE_ROW_ACTIONS, lpnTooltip, type EditLine } from "../editLine";

const props = defineProps<{
  line: EditLine;
  isTemplate: boolean;
  /** Dernière ligne, vide : la ligne de saisie (ni déplaçable, ni sélectionnable). */
  isEntry: boolean;
  selected: boolean;
  dragging: boolean;
  dropBefore: boolean;
  /** Montant d'une ligne de sous-total. */
  subtotal?: number;
  /** Propositions des recherches, seulement pour la ligne où l'on cherche. */
  productSuggestions: ProductHit[];
  enedisSuggestions: ProductHit[];
}>();

const a = inject(LINE_ROW_ACTIONS)!;

function onDiscountBlur() {
  props.line.discountText = props.line.discount ? editText(props.line.discount, 3) : "";
}

function setEnedis(v: string | ProductHit) {
  props.line.enedis_code = typeof v === "string" ? v : (v.enedis_code ?? "");
}

function setRef(v: string | ProductHit) {
  props.line.product_ref = typeof v === "string" ? v : v.ref;
}

type CustomPriceField = "public_price" | "discounted_price" | "threshold_price";
/** Prix d'une référence libre, validé en quittant le champ (Entrée aussi). */
function commitCustomPrice(field: CustomPriceField, e: Event) {
  a.onCustomPrice(props.line, field, (e.target as HTMLInputElement).value);
}
const blurOnEnter = (e: KeyboardEvent) => (e.target as HTMLInputElement).blur();
const CUSTOM_TIP = "Référence libre (hors catalogue) : code ENEDIS, désignation et prix se saisissent à la main";
</script>

<template>
  <tr
    v-if="line.kind !== 'item'"
    :data-key="line.key"
    :class="[`${line.kind}-row`, { selected, dragging, 'drop-before': dropBefore }]"
    @pointerdown="a.onRowPointerDown(line, $event)"
  >
    <td v-tooltip.left="'Glisser pour déplacer'" class="drag-cell drag-handle" @pointerdown="a.startDrag(line, $event)">
      <i class="pi pi-bars" />
    </td>
    <td class="select-cell">
      <input type="checkbox" :checked="selected" aria-label="Sélectionner la ligne" @click="a.onSelectClick(line, $event)" />
    </td>
    <template v-if="line.kind === 'text'">
      <td :colspan="isTemplate ? 5 : 13">
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
      <td v-if="!isTemplate"></td>
    </template>
    <!-- Titre : sépare le devis en paragraphes (gras, rouge ; plus grand sur le PDF). -->
    <template v-else-if="line.kind === 'title'">
      <td :colspan="isTemplate ? 5 : 13">
        <InputText
          :id="`text-${line.key}`"
          v-model="line.designation"
          fluid
          size="small"
          placeholder="Titre…"
          class="title-line"
        />
      </td>
      <td v-if="!isTemplate"></td>
    </template>
    <template v-else>
      <td :colspan="isTemplate ? 4 : 10">
        <InputText v-model="line.designation" fluid size="small" class="subtotal-label" />
      </td>
      <td v-if="!isTemplate" class="num subtotal-amount">{{ formatEuro(subtotal) }}</td>
      <td></td>
      <td v-if="!isTemplate"></td>
      <td v-if="!isTemplate"></td>
    </template>
    <td class="remove-cell">
      <Button icon="pi pi-times" text rounded size="small" severity="secondary" @click="a.removeLine(line)" />
    </td>
  </tr>
  <tr
    v-else
    :data-key="line.key"
    :class="{ selected, 'option-row': line.is_option, dragging, 'drop-before': dropBefore }"
    @pointerdown="a.onRowPointerDown(line, $event)"
  >
    <td
      v-if="!isEntry"
      v-tooltip.left="'Glisser pour déplacer'"
      class="drag-cell drag-handle"
      @pointerdown="a.startDrag(line, $event)"
    >
      <i class="pi pi-bars" />
    </td>
    <td v-else class="drag-cell"></td>
    <td class="select-cell">
      <input
        v-if="!isEntry"
        type="checkbox"
        :checked="selected"
        aria-label="Sélectionner la ligne"
        @click="a.onSelectClick(line, $event)"
      />
    </td>
    <td>
      <AutoComplete
        :model-value="line.enedis_code ?? ''"
        :input-id="`enedis-${line.key}`"
        :suggestions="enedisSuggestions"
        option-label="enedis_code"
        :delay="200"
        :min-length="2"
        :show-empty-message="false"
        fluid
        size="small"
        placeholder="Code…"
        input-class="mono enedis-input"
        @update:model-value="setEnedis"
        @complete="a.searchEnedis($event, line)"
        @option-select="a.selectEnedis(line, $event.value)"
        @keydown.enter="a.commitEnedis(line, true)"
        @keydown.tab="a.validateAndGoToQuantity(line, 'enedis', $event)"
        @blur="a.commitEnedis(line, false)"
      >
        <template #option="{ option }">
          <div class="opt">
            <span class="mono">{{ option.enedis_code }}</span>
            <span class="mono muted">{{ option.ref }}</span>
            <span class="muted">{{ option.designation }}</span>
          </div>
        </template>
      </AutoComplete>
    </td>
    <td v-tooltip.top="isCustom(line) ? CUSTOM_TIP : ''">
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
        :input-class="isCustom(line) ? 'custom-ref' : undefined"
        :invalid="!!line.error"
        @update:model-value="setRef"
        @complete="a.searchProducts($event, line)"
        @option-select="a.commitRef(line)"
        @blur="a.onRefBlur(line)"
        @keydown.enter="a.commitRef(line)"
        @keydown.tab="a.validateAndGoToQuantity(line, 'ref', $event)"
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
        @update:model-value="(v: string | undefined) => a.onQtyInput(line, v ?? '')"
        @blur="a.onQtyCommit(line)"
        @keydown.enter="a.moveInColumn(line, 'qty', 1, $event)"
        @keydown.down="a.moveInColumn(line, 'qty', 1, $event)"
        @keydown.up="a.moveInColumn(line, 'qty', -1, $event)"
      />
    </td>
    <template v-if="!isTemplate">
      <!-- Référence libre : prix public et prix remisé saisis à la main. -->
      <template v-if="isCustom(line)">
        <td v-for="field in ['public_price', 'discounted_price'] as const" :key="field">
          <div class="euro-input">
            <InputText
              :model-value="editText(line[field], 2)"
              inputmode="decimal"
              autocomplete="off"
              placeholder="–"
              fluid
              size="small"
              class="num"
              @change="commitCustomPrice(field, $event)"
              @keydown.enter="blurOnEnter"
            />
            <span>€</span>
          </div>
        </td>
      </template>
      <template v-else>
        <td class="num muted">{{ formatUnitPrice(line.public_price) }}</td>
        <!-- En gras : le prix qui a fixé le PU HT (le plus bas des deux, ou la liste forcée). -->
        <td class="num ref-price" :class="{ used: line.price_source?.startsWith('Public') }">
          {{ formatUnitPrice(line.discounted_price) }}
        </td>
      </template>
      <td
        v-tooltip.top="lpnTooltip(line)"
        class="num ref-price"
        :class="{ used: line.lpn_price != null && line.price_source === line.lpn_list }"
      >
        {{ formatUnitPrice(line.lpn_price) }}
      </td>
      <!-- Source du prix en infobulle (sur une 2e ligne, elle doublait la hauteur des lignes). -->
      <td
        v-tooltip.top="
          line.price_source ? `Prix : ${line.price_source}${isBelowThreshold(line) ? ' · sous le prix seuil' : ''}` : ''
        "
      >
        <div class="euro-input">
          <InputText
            :id="`price-${line.key}`"
            :model-value="line.priceText"
            inputmode="decimal"
            autocomplete="off"
            fluid
            size="small"
            class="num"
            :invalid="isBelowThreshold(line)"
            @update:model-value="(v: string | undefined) => a.onPriceInput(line, v ?? '')"
            @blur="a.onPriceBlur(line)"
            @keydown.enter="a.moveInColumn(line, 'price', 1, $event)"
            @keydown.down="a.moveInColumn(line, 'price', 1, $event)"
            @keydown.up="a.moveInColumn(line, 'price', -1, $event)"
          />
          <span v-if="line.product_ref">€</span>
        </div>
      </td>
      <td class="select-cell">
        <input
          v-if="line.product_ref"
          v-model="line.price_locked"
          v-tooltip.top="line.price_locked ? 'Prix verrouillé' : ''"
          type="checkbox"
          aria-label="Verrouiller le prix"
        />
      </td>
      <!-- Remise en % : le symbole suit la valeur, comme le € du PU HT. -->
      <td>
        <div class="euro-input">
          <InputText
            :model-value="line.discountText"
            inputmode="decimal"
            autocomplete="off"
            placeholder="–"
            fluid
            size="small"
            class="num"
            @update:model-value="(v: string | undefined) => a.onDiscountInput(line, v ?? '')"
            @blur="onDiscountBlur"
          />
          <span v-if="line.discountText.trim()">%</span>
        </div>
      </td>
      <td class="num">{{ line.product_ref && line.quantity != null ? formatEuro(lineTotal(line)) : "" }}</td>
    </template>
    <td class="select-cell">
      <input v-if="!isEntry" v-model="line.is_option" type="checkbox" aria-label="Ligne en option" />
    </td>
    <td v-if="!isTemplate" class="select-cell">
      <input
        v-if="line.product_ref"
        v-model="line.hide_price"
        v-tooltip.top="line.hide_price ? 'Prix masqué sur le PDF' : ''"
        type="checkbox"
        aria-label="Masquer le prix sur le PDF"
      />
    </td>
    <td v-if="!isTemplate && isCustom(line)" class="seuil">
      <div class="euro-input">
        <InputText
          :model-value="editText(line.threshold_price, 2)"
          inputmode="decimal"
          autocomplete="off"
          placeholder="–"
          fluid
          size="small"
          class="num"
          :invalid="isBelowThreshold(line)"
          @change="commitCustomPrice('threshold_price', $event)"
          @keydown.enter="blurOnEnter"
        />
        <span>€</span>
      </div>
    </td>
    <td v-else-if="!isTemplate" class="num seuil" :class="{ warn: isBelowThreshold(line) }">
      {{ formatUnitPrice(line.threshold_price) }}
    </td>
    <td class="remove-cell">
      <Button
        v-if="line.product_ref"
        icon="pi pi-times"
        text
        rounded
        size="small"
        severity="secondary"
        @click="a.removeLine(line)"
      />
    </td>
  </tr>
</template>

<style scoped>
/* Propositions des recherches : affichées hors du tableau (à la racine de la page), donc hors de
   portée des styles du tableau de QuoteEditView. */
.opt {
  display: flex;
  gap: 0.75rem;
  justify-content: space-between;
  width: 100%;
}
</style>
