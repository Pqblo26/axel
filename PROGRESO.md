# Progreso del revival de Konofan

Fork de [Axel](https://github.com/Assasans/axel) (servidor de fans de *KonoSuba: Fantastic Days*) para jugar en local con servidor propio, controlar banners, probabilidades, eventos y kits, y añadir contenido nuevo. Uso personal.

_Última actualización: 2026-11-11_

## Estado actual

Fase 3–4 (redirigir el cliente y servidor mínimo): el servidor está preparado para montarse en el PC de Pablo (Windows + WSL2 + MuMu), falta la primera prueba con el juego real.

## Qué funciona (heredado de Axel)

- Tutorial, creación de cuenta y perfil.
- Misiones principales, libres y de fama con recompensas; historias (también colaboraciones).
- Equipo, subir personajes, mejora de objetos, tienda, mazmorras, gacha diario, multijugador básico.
- Cliente stub (`konofd-client-stub` v1.1.0, global 4.11.5) sin LIAPP, con URL del servidor configurable sin recompilar.

## Qué hemos añadido

- **Gacha con probabilidades reales** (rama `gacha-probabilidades-reales`): las tiradas usan `gacha.rates`; la 10.ª de una multi usa la tabla de garantía. Corregido el fallo de `gacha_rate` con `bonus_rates`.
- **Instalación local en Windows** (rama `instalacion-windows-mumu`): `scripts/konofan/setup-wsl.sh`, `scripts/konofan/mirror-assets.sh` y la guía [docs/konofan/INSTALAR-WINDOWS-MUMU.md](docs/konofan/INSTALAR-WINDOWS-MUMU.md). Probado de punta a punta en Ubuntu 24.04 (PostgreSQL 16, nginx, TLS), sin el juego.

## Qué falta

- Primera prueba real en MuMu (red, certificado, carga de recursos).
- El gacha no cobra gemas/tickets (`money_type` se ignora) ni guarda historial (`gacha_log` es un ejemplo fijo).
- Los banners están escritos a mano en `gacha_info()` (`src/api/gacha.rs`); hay que pasarlos a la base de datos para poder editarlos.
- Endpoints todavía en stub (~100 avisos `encountered stub` en el código): sorteo de eventos, compras que fallan por el cliente, vincular cuenta, etc.
- El progreso del menú principal no se guarda.
- Herramientas propias (editor de banners, probabilidades, eventos y personajes).

## Decisiones tomadas

- **2026-11-11 — Axel como base (fork), no solo como referencia.** Ya resuelve el protocolo, la firma JWT RS256 (RSA-1024) y LIAPP; empezar de cero repetiría meses de ingeniería inversa.
- **2026-11-11 — Servidor en WSL2 (Ubuntu 24.04) con red en espejo.** Axel y sus scripts están pensados para Linux; con `networkingMode=mirrored` el puerto 443 de WSL se ve desde Windows y MuMu sin redirecciones.
- **2026-11-11 — Dominio `konofan.local` y certificado propio instalado como certificado de sistema en MuMu.** El cliente (Android 12+, OkHttp) no confía en certificados de usuario.
- **2026-11-11 — Recursos:** de momento se sirven desde el espejo de Assasans (`upstream-url`); `mirror-assets.sh` hace la copia local (~16 GiB) para no depender de él.
- La clave RSA de firma (`key.pem`) es la pública del repo de Axel; para uso local da igual. Si algún día se comparte el servidor, generar una propia (ver `SERVER-SETUP.md`).

## Recursos externos

- Espejo de Assasans: <https://smb.assasans.dev/konofd/> (APKs 3.8.7–4.11.6, bundles, banners, `gacha_infos/gacha_rates`, `global-metadata` descifrado). rsync: `rsync://aqua.assasans.dev/konofd/`.
- Cliente stub: <https://github.com/Assasans/konofd-client-stub/releases>.

## Siguientes pasos

1. Pablo: seguir la guía de instalación y llegar al menú principal en MuMu.
2. Probar una tirada en un banner con probabilidades importadas y revisar el log `gacha draw`.
3. Pasar los banners de `gacha_info()` a la base de datos y cobrar la moneda del gacha.
4. Editor de banners y probabilidades.
