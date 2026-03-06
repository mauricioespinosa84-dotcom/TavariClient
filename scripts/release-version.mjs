import { readFile, writeFile } from "node:fs/promises";
import path from "node:path";

const rootDir = process.cwd();
const nextVersion = process.argv[2]?.trim();

const semverPattern =
  /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/;

if (!nextVersion || !semverPattern.test(nextVersion)) {
  console.error("Uso: npm run release:version -- 1.0.1");
  process.exit(1);
}

const files = {
  packageJson: path.join(rootDir, "package.json"),
  packageLock: path.join(rootDir, "package-lock.json"),
  tauriConfig: path.join(rootDir, "src-tauri", "tauri.conf.json"),
  cargoToml: path.join(rootDir, "src-tauri", "Cargo.toml")
};

const updateJsonVersion = async (filePath, transform) => {
  const raw = await readFile(filePath, "utf8");
  const json = JSON.parse(raw);
  transform(json);
  await writeFile(filePath, `${JSON.stringify(json, null, 2)}\n`, "utf8");
};

await updateJsonVersion(files.packageJson, (json) => {
  json.version = nextVersion;
});

await updateJsonVersion(files.packageLock, (json) => {
  json.version = nextVersion;
  if (json.packages?.[""]) {
    json.packages[""].version = nextVersion;
  }
});

await updateJsonVersion(files.tauriConfig, (json) => {
  json.version = nextVersion;
});

const cargoTomlRaw = await readFile(files.cargoToml, "utf8");
const cargoTomlNext = cargoTomlRaw.replace(
  /^version = ".*"$/m,
  `version = "${nextVersion}"`
);

if (cargoTomlRaw === cargoTomlNext) {
  console.error("No se pudo actualizar la version en src-tauri/Cargo.toml");
  process.exit(1);
}

await writeFile(files.cargoToml, cargoTomlNext, "utf8");

console.log(`Version actualizada a ${nextVersion}`);
console.log("Siguiente paso sugerido:");
console.log(`git commit -am "release: v${nextVersion}"`);
console.log(`git tag v${nextVersion}`);
console.log("git push && git push --tags");
