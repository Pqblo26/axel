# Instalar el revival de Konofan en Windows con MuMu

Esquema final:

```
MuMu (cliente stub)  --HTTPS-->  Windows :443  ==(WSL2, red en espejo)==>  nginx  -->  Axel :2020 / :2021  -->  PostgreSQL
```

- **WSL2 + Ubuntu**: Axel, sus scripts y la documentación original están pensados para Linux. Así usamos exactamente lo que ya funciona en vez de portarlo a Windows.
- **nginx** pone el HTTPS delante de Axel (el juego solo habla HTTPS) y junta las dos partes del servidor en un solo dominio: `/static/` y `/api/`.
- **`konofan.local`**: un nombre propio en vez de una IP, para que el certificado no dependa de la IP del PC. MuMu lo resuelve con una línea en su `/etc/hosts`.
- **Certificado de sistema en MuMu**: el cliente (OkHttp, Android 12+) solo confía en certificados de *sistema*, no en los que instala el usuario. Por eso hace falta root y el disco de sistema en modo escritura.

Tiempo aproximado: 30–45 minutos, casi todo esperando descargas y la compilación.

---

## 1. WSL2 con Ubuntu (Windows)

1. Abre **PowerShell como administrador** y ejecuta:

   ```powershell
   wsl --install -d Ubuntu-24.04
   ```

2. **Reinicia** el PC. Al volver se abre Ubuntu y te pide un usuario y una contraseña de Linux (los que quieras; la contraseña es la de `sudo`).

3. Comprueba tu versión de Windows con `winver`. La red en espejo necesita **Windows 11 22H2 o posterior**. Si tienes Windows 10, dímelo y lo hacemos con `netsh portproxy`.

4. Crea el fichero `%USERPROFILE%\.wslconfig` (por ejemplo `C:\Users\<tú>\.wslconfig`) con este contenido:

   ```ini
   [wsl2]
   networkingMode=mirrored
   ```

   Con esto los puertos que abre Linux (443 de nginx) aparecen directamente en las IPs de Windows, sin redirecciones a mano.

5. En **PowerShell como administrador**, permite conexiones entrantes a WSL y reinícialo:

   ```powershell
   Set-NetFirewallHyperVVMSetting -Name '{40E0AC32-46A5-438A-A0B2-2B479E8F2E90}' -DefaultInboundAction Allow
   wsl --shutdown
   ```

## 2. Servidor dentro de Ubuntu (WSL)

Abre **Ubuntu** desde el menú Inicio y ejecuta:

```bash
git clone https://github.com/Pqblo26/konofan-revival ~/axel
cd ~/axel
./scripts/konofan/setup-wsl.sh
```

El script instala las dependencias, crea la base de datos y aplica las migraciones, genera el certificado, configura nginx, compila Axel y descarga e importa las probabilidades reales del gacha. Te pedirá la contraseña de `sudo` al principio. Si algo falla, puedes volver a lanzarlo: lo que ya está hecho se salta.

Al terminar, deja el servidor arrancado en esa ventana (aquí verás los registros de todo lo que pide el juego):

```bash
cd ~/axel && ./target/release/axel
```

Espera a ver `api server started` y, **en otra ventana de Ubuntu**, comprueba que responde:

```bash
curl --noproxy '*' --cacert ~/axel/certs/konofan-ca.crt --resolve konofan.local:443:127.0.0.1 https://konofan.local/static/public.pem
```

Debe salir un bloque `-----BEGIN PUBLIC KEY-----`.

El script ha copiado el certificado para MuMu a `C:\Users\<tú>\konofan\XXXXXXXX.0` (el nombre exacto sale al final del script; lo llamo `XXXXXXXX.0` abajo). Es el de la **autoridad propia (CA)**, no el del servidor: la CA dura 10 años y firma un certificado de servidor que dura 397 días (Chrome no acepta más). Para renovar el del servidor basta con volver a ejecutar `setup-wsl.sh`; en MuMu no hay que tocar nada.

**Copia local de los recursos (opcional, recomendado):** de momento los recursos del juego (~16 GiB) se piden al espejo de Assasans cuando hacen falta. Para no depender de él:

```bash
~/axel/scripts/konofan/mirror-assets.sh
```

Se puede cortar con Ctrl+C y reanudar después.

## 3. Preparar MuMu

1. En MuMu: **Ajustes → Otros → Permiso de root: activado**, y **Ajustes → Disco → Disco del sistema: lectura y escritura** (los nombres pueden variar un poco según la versión). Reinicia MuMu.

2. Busca el `adb` de MuMu. En PowerShell:

   ```powershell
   Get-ChildItem "C:\Program Files\Netease" -Recurse -Filter adb.exe -ErrorAction SilentlyContinue | Select-Object FullName
   ```

   El puerto ADB aparece en el **Multi-instancia** de MuMu (suele ser `16384`). Desde la carpeta de ese `adb.exe`:

   ```powershell
   .\adb.exe connect 127.0.0.1:16384
   .\adb.exe push "$env:USERPROFILE\konofan\XXXXXXXX.0" /sdcard/Download/
   .\adb.exe shell
   ```

3. Dentro del `adb shell` (cambia el nombre del fichero por el tuyo):

   ```sh
   su
   mount -o rw,remount /          # si falla: mount -o rw,remount /system
   cp /sdcard/Download/XXXXXXXX.0 /system/etc/security/cacerts/
   chmod 644 /system/etc/security/cacerts/XXXXXXXX.0
   echo "10.0.2.2 konofan.local" >> /system/etc/hosts
   exit
   exit
   ```

   Y reinicia el emulador: `.\adb.exe reboot`

4. **Comprobación:** abre el navegador de MuMu y entra en `https://konofan.local/static/public.pem`. Tiene que mostrar la clave **sin aviso de certificado**. `10.0.2.2` es la dirección con la que MuMu ve el `localhost` de Windows. No uses la IP de la red del PC: con la red en espejo de WSL, Windows no reenvía a WSL lo que llega a su propia IP.

## 4. Instalar y abrir el juego

1. Descarga `jp.assasans.konofd.stub-v1.1.0-global-4.11.5-release.apk` (124 MB) de <https://github.com/Assasans/konofd-client-stub/releases/tag/v1.1.0>.
2. Arrástralo a la ventana de MuMu para instalarlo. El APK es solo arm64; si MuMu dice que no es compatible, usa una instancia de Android 12 de 64 bits.
3. Abre **KonoSuba: FD (stub)** y pon como servidor `https://konofan.local/static/` **con la barra final** (el juego pega `versions/...` detrás). El lanzador a veces la olvida al reabrirlo; nginx redirige `/staticversions/...` a `/static/versions/...`, así que también funciona sin ella. Como *Patching method* elige **LoadMetadataFile (And64InlineHook)**: los métodos de hooking/escaneo de memoria se cierran al instante en MuMu porque su traductor ARM (libhoudini) los bloquea. Pulsa Check, acepta la clave pública (la descarga del propio servidor) y Launch Game.
4. La primera carga descarga muchos recursos; es normal que tarde.

## Si algo falla

Mándame:

- en qué paso se queda (una captura ayuda),
- las últimas líneas de la ventana donde corre `./target/release/axel`.

Con eso suele bastar para saber si es red, certificado o un endpoint que falta en Axel.
