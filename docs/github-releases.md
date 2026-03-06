# GitHub Releases y Auto Update

Este proyecto ya quedo preparado para que GitHub Releases publique:

- instalador de Windows
- archivo `.sig`
- `latest.json`

Y para que el launcher use ese `latest.json` automaticamente al arrancar.

## 1. Generar las llaves del updater

Hazlo una sola vez en tu maquina:

```powershell
cd C:\Users\mauri\OneDrive\Documents\LAUNCHERTEST
npm run tauri signer generate -- -w "$HOME/.tauri/tavari-client.key"
```

Eso te va a entregar:

- una llave privada
- una llave publica

La llave privada firma los instaladores.
La llave publica la usa el launcher para validar los updates.

## 2. Crear los secrets en GitHub

En tu repositorio de GitHub crea estos secrets:

- `TAURI_SIGNING_PRIVATE_KEY`
  Valor: el contenido completo de la llave privada
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`
  Valor: la password de la llave, si le pusiste una
- `TAURI_UPDATER_PUBKEY`
  Valor: el contenido completo de la llave publica

## 3. Endpoint que usara el launcher

El workflow compila el launcher con este endpoint por defecto:

```text
https://github.com/mauricioespinosa84-dotcom/TavariClient/releases/latest/download/latest.json
```

En GitHub Actions eso se llena automaticamente con:

```text
https://github.com/${{ github.repository }}/releases/latest/download/latest.json
```

Asi los usuarios no tienen que configurar el updater manualmente.

## 4. Estructura esperada del release

Cada release publicado por el workflow tendra assets parecidos a estos:

```text
tavari-client_1.0.1_windows_x86_64-setup.exe
tavari-client_1.0.1_windows_x86_64-setup.exe.sig
latest.json
```

Si GitHub Action genera MSI adicional, tambien puede subirlo, pero el `latest.json`
quedara prefiriendo NSIS por esta opcion:

```yml
updaterJsonPreferNsis: true
```

## 5. Formato de latest.json

Tauri espera un JSON asi:

```json
{
  "version": "1.0.1",
  "notes": "Release notes",
  "pub_date": "2026-03-06T08:00:00Z",
  "platforms": {
    "windows-x86_64": {
      "signature": "CONTENIDO_DEL_SIG",
      "url": "https://github.com/mauricioespinosa84-dotcom/TavariClient/releases/download/v1.0.1/tavari-client_1.0.1_windows_x86_64-setup.exe"
    }
  }
}
```

No necesitas generarlo a mano: `tauri-action` lo sube solo.

## 6. Flujo de release sin tocar archivos manualmente

Cuando quieras sacar una version nueva:

```powershell
cd C:\Users\mauri\OneDrive\Documents\LAUNCHERTEST
npm run release:version -- 1.0.1
```

Eso actualiza:

- `package.json`
- `package-lock.json`
- `src-tauri/Cargo.toml`
- `src-tauri/tauri.conf.json`

Luego haces:

```powershell
git add .
git commit -m "release: v1.0.1"
git tag v1.0.1
git push
git push --tags
```

Con eso el workflow:

1. compila el instalador
2. firma los assets
3. genera `latest.json`
4. crea el GitHub Release

## 7. Como lo usa el launcher

El launcher carga el updater al iniciar y:

- revisa `latest.json`
- muestra progreso en la splashscreen
- descarga e instala
- se reinicia solo

Si el usuario ya tenia `settings.json` sin updater configurado, el launcher tambien
puede tomar por defecto los valores embebidos en el build.
