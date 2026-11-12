# Progreso del revival de Konofan

Fork de [Axel](https://github.com/Assasans/axel) (servidor de fans de *KonoSuba: Fantastic Days*) para jugar en local con servidor propio, controlar banners, probabilidades, eventos y kits, y añadir contenido nuevo. Uso personal.

_Última actualización: 2026-11-12_

## Estado actual

Fase 3–4 (redirigir el cliente y servidor mínimo). El 2026-11-12 quedó montado en el PC de Pablo: WSL2 Ubuntu 24.04 con Axel, PostgreSQL (116.271 probabilidades importadas) y nginx; MuMu con el certificado de sistema y `10.0.2.2 konofan.local` en su hosts (copia del original en `/system/etc/hosts.orig`). Falta la primera partida real.

## Qué funciona (heredado de Axel)

- Tutorial, creación de cuenta y perfil.
- Misiones principales, libres y de fama con recompensas; historias (también colaboraciones).
- Equipo, subir personajes, mejora de objetos, tienda, mazmorras, gacha diario, multijugador básico.
- Cliente stub (`konofd-client-stub` v1.1.0, global 4.11.5) sin LIAPP, con URL del servidor configurable sin recompilar.

## Qué hemos añadido

- **Gacha con probabilidades reales** (rama `gacha-probabilidades-reales`): las tiradas usan `gacha.rates`; la 10.ª de una multi usa la tabla de garantía. Corregido el fallo de `gacha_rate` con `bonus_rates`. Historial de tiradas real (`gacha.user_draws`, endpoint `gachalog`).
- **Arreglos tras la primera partida** (rama `claude/project-thread-hmlqra`, 2026-11-12):
  - Carrusel de la portada: el servidor reactiva los banners de `config/home-banners.toml` (IDs de `master/banner.json`); todos habían caducado al cerrar el juego.
  - Buzón de regalos real: tabla `user_presents`, contador correcto en Gifts, historial (Gift Log), regalo de bienvenida una vez por jugador y `scripts/konofan/enviar-regalo.sh` para mandar regalos. Miembros/armas/accesorios aún no se pueden regalar.
  - Anuncios propios en Note: tabla `news` y páginas de detalle en `/static/news/<id>.html`.
  - Los recursos locales (`resources-root`, `/srv/konofd`) por fin se usan: `serve_dir_with_fallback` servía desde la ruta de la URL y todo acababa yendo al espejo.
- **Segunda tanda de arreglos** (2026-11-12):
  - Jobs (expediciones) reales: tabla `user_expeditions`; cada 864 s un «turno» (máx. 50, 12 h) con 400 eris, 2 de afinidad y una tirada de `expedition_drop`; Accept All, asignar y quitar personaje.
  - Exchange real: cobra las medallas (primer canje con precio `_first`), da el contenido del pack y respeta los límites (NUM/MONTH/WEEK, tabla `user_exchanges`). Las medallas de diosa y los tickets de salto pasan a `user_items` (antes se sumaban en cada login sin guardarse).
  - Rombos rojos: el servidor ya no manda avisos de «nuevo» fijos a 1; el de Jobs se calcula (algún trabajo al máximo).
  - Shop: los productos de pago real (con precio de Google Play) se quedan caducados para que la tienda no espere a Google Play Billing; solo quedan los gratis.
- **Tercera tanda: monedas y consumos** (2026-11-12):
  - Gemas (de pago y gratis), eris y estamina se guardan en `user_wallet` (migración `20261112000004`). Cada cuenta empieza con lo que tenía hasta ahora (85 720 eris, 100 000 + 1 000 000 gemas, 419 de estamina).
  - La estamina se recarga 1 punto cada 3 minutos hasta el máximo del rango (200 en rango 45, maestro `exp`); por encima del máximo no se recarga.
  - Main, Free, Event y Fame gastan la estamina del nivel al empezar (Fame la multiplica por `cost_ratio`). El boss de evento gasta sus tickets (`bossticket` × multiplicador).
  - Cada victoria da las eris y la afinidad del nivel (`money`, `intimacy_exp`) y se guardan; antes la afinidad solo se mostraba.
  - Skip real en Main, Free y Event: gasta tickets de salto y estamina por cada pasada y da las eris y los objetos del nivel. Si falta algo, no gasta ni da nada.
  - `buy`: comprar estamina (200 por 100 gemas, 30 al día), Fame Potions, retos de mazmorra y re-sorteos con gemas o eris; se gastan antes las gemas gratis. Compras guardadas en `user_shop_purchases` para los límites diarios.
  - Regalos, canjes y Jobs suman o cobran las eris y gemas en la base de datos.
  - Las estrellas de las misiones de evento salen doradas (para poder saltarlas).
  - Métodos que el juego llamaba y no existían (cerraban la sesión con «Connection Error»): `itemuse` (usar carne de estamina: 100 o 10 por unidad, maestro `stamina_item`), `fame_supplement_buy` (comprar Fame Potions desde la Fame Quest) y `partycopy` (copiar un equipo en otro).
- **Cuarta tanda: combate** (2026-11-12):
  - Las estadísticas de batalla salían multiplicadas por 20: `MinMaxRange::interpolate` (src/member.rs) tenía activo un multiplicador de depuración heredado de Axel (`POST_MULTIPLIER = 20.0`). Con agilidad y suerte ×20, los enemigos casi nunca actuaban en Auto, fallaban siempre y todo moría de un golpe.
  - Curva de crecimiento real: `min + (max − min) × ((nivel − 1) / 59)^1,2`, que cuadra con los miembros de nivel 3 y 4 de los datos capturados del servidor original (Kazuma nivel 4: 277 de vida).
  - Cada miembro lleva su propia habilidad pasiva (`passiveskill1`) y su resistencia a atributos (`resist_attr`); antes todos llevaban las de Kazuma.
  - PWR de los equipos: cada fila iba con 12 300 fijo (5 × 12 300 = 61 500). Ahora se calcula: base = ⌊0,35·vida + 3·(ataque + ataque mágico) + 1,5·(defensa + def. mágica)⌋; titular = base + 90; suplente = ⌊base × 0,3⌋ (0,4 si es el mismo personaje). Da exactos los valores de los 20 miembros de los datos capturados.
  - Afinidad: el login mandaba cambiados el nivel (`rank`) y los puntos (`rank_progress`); la afinidad sí se guardaba.
- **Quinta tanda: avisos y equipos** (2026-11-12):
  - Friends: el Greeting Log saltaba siempre porque el login mandaba la versión de Axel como saludo (aviso tipo 7, clave 27) y `greeting_list` devolvía un saludo falso de Megumin. Ya no hay saludos (no hay más jugadores).
  - Misiones: el contador rojo venía de avisos fijos (clave 3 = 2, clave 13 = 7) y de un «misión completada» (11210001) en cada combate. Van a 0 hasta que las misiones sean reales.
  - Equipos: Reset vacía el equipo y Suggest lo llena con los personajes más fuertes (uno por personaje: 5 titulares y luego suplentes).
  - `scorechallenge_log` ya no falla cuando el juego no manda `event_id`.
- **Sexta tanda: progreso real** (2026-11-12):
  - Las cuentas nuevas ya no nacen con rango 45, todo completado y todos los personajes. Empiezan con Kazuma, Aqua, Megumin y Darkness; el gacha del tutorial guarda de verdad sus 10 personajes. Monedas iniciales: 10 000 eris y 3000 cuarzos gratis (más los regalos de bienvenida, ahora con 20 tickets de salto). Ya no se regalan todas las armas, 60 000 pociones ni 200 000 medallas.
  - Rango y experiencia del jugador guardados (`user_progress`, maestro `exp`): la estamina máxima sale del rango y al subir de rango se recarga.
  - Progreso de misiones (`user_quest_progress`): la historia se desbloquea con `unlock_clearstage` (partes, capítulos y fases), se guardan las coronas y cada corona da su recompensa la primera vez (`mainmission_reward{1,2,3}_packid`; la 1-1 da a Iris). Hard: 3 intentos al día; Expert: 1. Solo se puede saltar una fase con las 3 coronas, y los saltos gastan intentos y dan experiencia.
  - Los combates dan experiencia a los titulares y se guarda (antes 230 fijo y no se guardaba). Perder ya no da recompensas.
  - Free Quest: solo salen las zonas fijas y las del día de la semana (hora de Japón), con sus intentos diarios. Las zonas de eventos antiguos (las de nombre en japonés) ya no aparecen.
  - Miembros desde regalos, canjes, packs y coronas (`src/user/grant.rs`), con su personaje y habilidades.
  - `scripts/konofan/reiniciar-cuenta.sh`: devuelve una cuenta al estado de después del tutorial.
- **Séptima tanda: Dungeons y Battle Arena** (2026-11-12):
  - Dungeons reales (`user_dungeon_runs`, `user_dungeon_clears`): solo sale la mazmorra de evento de la semana (rota cada jueves a medianoche UTC), con sus 3 zonas desbloqueadas en orden (`unlock_id`). Cada zona tiene 5 pisos con los enemigos y la vida de `battle_wave`/`battle_enemy`; la vida de tu equipo y la de los enemigos se guarda entre pisos y entre intentos. Bendiciones reales (3 a elegir, re-sorteo, curar y revivir). Al ganar el último piso se da el rango (3 = sin bajas, según `dungeon_clear_rank`) y sus recompensas; con la zona superada se puede saltar con tickets. Las mazmorras normales (11–14) no salen porque no tienen imágenes en el cliente.
  - Battle Arena real (`user_score_challenge_best`): 3 intentos al día (hora de Japón) compartidos entre Normal, Hard y Very Hard; la práctica no gasta. Se guarda la mejor puntuación de cada modo; la total suma las tres y sale en el ranking (solo tú). Cada combate da 10 Arena Medals y cada umbral de puntuación total su recompensa una vez (`scorechallenge_reward_achievement`). Sin intentos, el combate cuenta como práctica.
  - El jefe de la Arena no muere a propósito: la puntuación es el daño hecho en el tiempo límite.
- **Instalación local en Windows** (rama `instalacion-windows-mumu`): `scripts/konofan/setup-wsl.sh`, `scripts/konofan/mirror-assets.sh` y la guía [docs/konofan/INSTALAR-WINDOWS-MUMU.md](docs/konofan/INSTALAR-WINDOWS-MUMU.md). Probado de punta a punta en Ubuntu 24.04 (PostgreSQL 16, nginx, TLS), sin el juego.

## Qué falta

- El **+ de las gemas** se queda cargando: el juego espera a Google Play Billing y la petición nunca llega al servidor. Solo se arregla en el cliente; de momento, gemas por el buzón.
- Armas y accesorios no se pueden dar desde regalos ni canjes todavía.
- Sin comprobar en el juego: que el cliente acepte rango y experiencia absolutos tras el combate (se mandan como las monedas) y que las fases bloqueadas se oculten bien.
- Fame Potions: se pueden comprar, pero `use_supplement_num` al empezar una Fame Quest todavía no las gasta.
- Boss de evento: falta dar medallas (los maestros no dicen cuántas) e intentos limitados por día (`limit`, Challenges).
- Battle Arena EX: sin implementar («Not available»). Los retos de mazmorra (`DungeonChallenge`) se compran pero aún no se gastan.
- El gacha no cobra gemas/tickets (`money_type` se ignora).
- Los banners están escritos a mano en `gacha_info()` (`src/api/gacha.rs`); hay que pasarlos a la base de datos para poder editarlos.
- Endpoints todavía en stub (~100 avisos `encountered stub` en el código): sorteo de eventos, compras que fallan por el cliente, vincular cuenta, etc.
- Herramientas propias: **panel web** (Pablo lo quiere) para ver jugadores, tiradas y registros, y editar banners, probabilidades, eventos y personajes.

## Fallos encontrados jugando (2026-11-12)

| # | Fallo | Causa | Arreglo |
|---|-------|-------|---------|
| 1 | La portada no muestra el carrusel de eventos ni el botón Events | Los banners de la portada salen del maestro `banner` (1364 filas con `start_at`/`end_at`) y todos caducaron al cerrar el juego. Tampoco hay eventos activos (solo se reactiva el 24011 en `event_config`) | Parchear `banner`/`event_config` en `master_all.rs` para activar los que elijamos (después, desde el panel) |
| 2 | Regalos: «6» notificaciones, un regalo fijo de 10 000 gemas («skebob») y «Unable to receive: Stamina ×500» | `src/api/present.rs` es un stub: devuelve siempre los mismos datos de ejemplo de Axel y no entrega nada | Buzón de regalos real (tabla, entregar objetos); útil también para el panel |
| 3 | Note (avisos) en blanco | Dos causas: (a) `static_server.rs` usaba la ruta de la URL en `ServeDir::new` en vez de la carpeta local, así que todo salía del espejo (y `help/` devolvía su listado «Eris»); (b) el WebView (Chromium) rechazaba el certificado autofirmado que hacía de CA y de servidor | **Arreglado** (2026-11-12): `ServeDir::new(local_root)` (en el PR del hilo de fallos) y CA propia (`5ad9c253.0` en MuMu) + certificado de servidor de 397 días. Confirmado en el juego |
| 4 | Help de Branch en blanco | Igual que el 3 | **Arreglado** con el 3 |
| 5 | El «+» de gemas se queda cargando | No llega ninguna petición al servidor: el cliente espera a Google Play Billing, que en MuMu no responde | Baja prioridad: en local las gemas se dan por regalos o panel |

Otros stubs vistos en el registro: misiones (`mission_list`, `mission_home`), trabajos/expediciones (`expedition_set`), mini eventos sorpresa, recompensas por anuncios, invitaciones multijugador, vincular cuenta (`idlink_confirm_google`), intercambio (`exchange`) y propuesta de equipo (`party_offer`).

## Decisiones tomadas

- **2026-11-11 — Axel como base (fork), no solo como referencia.** Ya resuelve el protocolo, la firma JWT RS256 (RSA-1024) y LIAPP; empezar de cero repetiría meses de ingeniería inversa.
- **2026-11-11 — Servidor en WSL2 (Ubuntu 24.04) con red en espejo.** Axel y sus scripts están pensados para Linux; con `networkingMode=mirrored` el puerto 443 de WSL se ve desde Windows y MuMu sin redirecciones.
- **2026-11-12 — MuMu entra por `10.0.2.2`** (el localhost de Windows visto desde el emulador), no por la IP de la LAN: con la red en espejo de WSL, Windows no reenvía a WSL el tráfico hacia su propia IP.
- **2026-11-11 — Dominio `konofan.local` y certificado propio instalado como certificado de sistema en MuMu.** El cliente (Android 12+, OkHttp) no confía en certificados de usuario.
- **2026-11-12 — CA propia que firma el certificado del servidor (397 días).** El certificado inicial hacía de CA y de servidor a la vez: el juego lo aceptaba, pero Chromium y el WebView (Note, Help) lo rechazaban en silencio. En MuMu solo se instala la CA, así que renovar el certificado del servidor no obliga a tocar el emulador.
- **2026-11-11 — Recursos:** de momento se sirven desde el espejo de Assasans (`upstream-url`); `mirror-assets.sh` hace la copia local (~16 GiB) para no depender de él.
- La clave RSA de firma (`key.pem`) es la pública del repo de Axel; para uso local da igual. Si algún día se comparte el servidor, generar una propia (ver `SERVER-SETUP.md`).
- **2026-11-12 — Banners de portada y anuncios configurables desde fuera del código** (`config/home-banners.toml`, tablas `news` y `user_presents`), pensando en el futuro panel web.

## Recursos externos

- Espejo de Assasans: <https://smb.assasans.dev/konofd/> (APKs 3.8.7–4.11.6, bundles, banners, `gacha_infos/gacha_rates`, `global-metadata` descifrado). rsync: `rsync://aqua.assasans.dev/konofd/`.
- Cliente stub: <https://github.com/Assasans/konofd-client-stub/releases>.

## Siguientes pasos

1. Pablo: seguir la guía de instalación y llegar al menú principal en MuMu.
2. Probar una tirada en un banner con probabilidades importadas y revisar el log `gacha draw`.
3. Pasar los banners de `gacha_info()` a la base de datos y cobrar la moneda del gacha.
4. Editor de banners y probabilidades.
