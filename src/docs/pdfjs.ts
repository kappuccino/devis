// pdfjs dans la fenêtre de l'appli : build « legacy », car le build standard de pdfjs 6 exige
// des API absentes du WebKit de macOS (ex. Math.sumPrecise). Le worker est bundlé localement (aucun CDN).
import "./polyfills.js";
import * as pdfjs from "pdfjs-dist/legacy/build/pdf.mjs";

pdfjs.GlobalWorkerOptions.workerPort = new Worker(new URL("./pdf-worker.js", import.meta.url), { type: "module" });

export const PDFJS_OPTIONS = {
  cMapUrl: "/pdfjs/cmaps/",
  cMapPacked: true,
  standardFontDataUrl: "/pdfjs/standard_fonts/",
  wasmUrl: "/pdfjs/wasm/",
  isEvalSupported: false,
  // Polices absentes du PDF : polices de secours de pdfjs (public/pdfjs/standard_fonts) plutôt que
  // celles du système, illisibles sur certains PDF anciens sous Windows.
  useSystemFonts: false,
};

export { pdfjs };
