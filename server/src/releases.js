import { createWriteStream } from "node:fs";
import { mkdir, readFile, rename, rm, stat } from "node:fs/promises";
import { join } from "node:path";
import { Transform } from "node:stream";
import { pipeline } from "node:stream/promises";
import { HttpError } from "./licence.js";

/**
 * Versions de l'appli, dans `<dataDir>/releases` :
 * - `latest.json` : manifeste lu par le module de mise à jour de l'appli (version, notes, url, signature) ;
 * - `<version>/<fichier>` : installeurs déposés par la CI.
 * La CI dépose d'abord les fichiers, puis `latest.json` : un poste ne voit jamais une version incomplète.
 */
const VERSION = /^\d{1,4}\.\d{1,4}\.\d{1,4}$/;
const FILE = /^[A-Za-z0-9][A-Za-z0-9._-]{0,150}$/;
const MAX_FILE = 300 * 1024 * 1024;
const MAX_MANIFEST = 64 * 1024;

export function createReleases(dataDir) {
  const root = join(dataDir, "releases");

  /** Chemin d'un installeur, ou erreur si le nom est invalide. */
  const filePath = (version, file) => {
    if (!VERSION.test(version) || !FILE.test(file) || file.includes("..")) throw new HttpError(400, "Nom de fichier invalide.");
    return join(root, version, file);
  };

  /** Écrit le flux dans `target` (fichier temporaire puis renommage), dans la limite de `max` octets. */
  async function save(stream, target, max) {
    await mkdir(join(target, ".."), { recursive: true });
    const tmp = `${target}.upload`;
    let size = 0;
    const limit = new Transform({
      transform(chunk, _enc, done) {
        size += chunk.length;
        done(size > max ? new HttpError(413, "Fichier trop volumineux.") : null, chunk);
      },
    });
    try {
      await pipeline(stream, limit, createWriteStream(tmp));
      await rename(tmp, target);
    } catch (e) {
      await rm(tmp, { force: true });
      throw e;
    }
    return size;
  }

  return {
    async saveFile(version, file, stream) {
      return { size: await save(stream, filePath(version, file), MAX_FILE) };
    },

    async saveManifest(stream) {
      const target = join(root, "latest.json");
      await save(stream, `${target}.new`, MAX_MANIFEST);
      try {
        const manifest = JSON.parse(await readFile(`${target}.new`, "utf8"));
        if (!VERSION.test(manifest?.version) || typeof manifest.platforms !== "object") throw new Error();
      } catch {
        await rm(`${target}.new`, { force: true });
        throw new HttpError(400, "latest.json invalide.");
      }
      await rename(`${target}.new`, target);
      return { ok: true };
    },

    /** Manifeste de la dernière version, ou null s'il n'y en a pas encore. */
    async manifest() {
      try {
        return await readFile(join(root, "latest.json"), "utf8");
      } catch (e) {
        if (e.code === "ENOENT") return null;
        throw e;
      }
    },

    /** Chemin d'un installeur existant, sinon null. */
    async existingFile(version, file) {
      let path;
      try {
        path = filePath(version, file);
      } catch {
        return null;
      }
      const s = await stat(path).catch(() => null);
      return s?.isFile() ? path : null;
    },
  };
}
