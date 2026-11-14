#!/usr/bin/env bash
# Monta Axel en local dentro de WSL2 (Ubuntu 24.04): dependencias, PostgreSQL,
# certificado TLS, nginx, configuración, compilación y probabilidades del gacha.
# Se puede ejecutar varias veces: lo que ya está hecho se salta.
#
# Uso (desde la carpeta del repositorio, dentro de WSL):
#   ./scripts/konofan/setup-wsl.sh
set -euo pipefail

DOMAIN="${KONOFAN_DOMAIN:-konofan.local}"
RESOURCES_ROOT="${KONOFAN_RESOURCES:-/srv/konofd}"
RSYNC_BASE="rsync://aqua.assasans.dev/konofd"
REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CERT_DIR="$REPO_DIR/certs"
DB_USER="$(id -un)"

step() { printf '\n\033[1;36m==> %s\033[0m\n' "$*"; }

step "1/9 Paquetes del sistema"
sudo apt-get update -q
# cmake + clang: needed to build boring-sys / aws-lc-sys (TLS crates) on a clean Ubuntu.
sudo apt-get install -y -q build-essential pkg-config cmake clang libclang-dev git curl rsync openssl \
  postgresql nginx python3-psycopg2 rustup

step "2/9 Rust"
# rustup from Ubuntu's repositories (avoids piping a script from the internet into sh)
rustup toolchain list 2>/dev/null | grep -q stable || rustup default stable
export PATH="$HOME/.cargo/bin:/usr/lib/rustup/bin:$PATH"
cargo --version

step "3/9 PostgreSQL (usuario '$DB_USER', base de datos 'axel')"
sudo systemctl enable --now postgresql
[ "$(sudo -u postgres psql -tAc "select 1 from pg_roles where rolname='$DB_USER'")" = 1 ] \
  || sudo -u postgres createuser "$DB_USER"
[ "$(sudo -u postgres psql -tAc "select 1 from pg_database where datname='axel'")" = 1 ] \
  || sudo -u postgres createdb axel --owner "$DB_USER"
printf 'y\n' | "$REPO_DIR/migrations/migrate.sh" "host=/var/run/postgresql dbname=axel user=$DB_USER" | tail -n 3

step "4/9 Carpeta de recursos del juego ($RESOURCES_ROOT)"
sudo mkdir -p "$RESOURCES_ROOT"
sudo chown "$DB_USER:$DB_USER" "$RESOURCES_ROOT"

step "5/9 Configuración (config/local.toml)"
if [ ! -f "$REPO_DIR/config/local.toml" ]; then
  cat > "$REPO_DIR/config/local.toml" <<TOML
[static-server]
bind-address = "127.0.0.1:2021"
public-url = "https://$DOMAIN/static/"
resources-root = "$RESOURCES_ROOT"
# Si falta un fichero en local, se pide al espejo de Assasans.
upstream-url = "https://smb.assasans.dev/konofd/"

[api-server]
bind-address = "127.0.0.1:2020"
public-url = "https://$DOMAIN/api/"

[database.pool]
host = "/var/run/postgresql"
user = "$DB_USER"
dbname = "axel"
TOML
  echo "Creado config/local.toml"
else
  echo "config/local.toml ya existe, no lo toco"
fi

step "6/9 Certificados TLS para $DOMAIN (CA propia + certificado de servidor)"
# Una autoridad propia (CA) firma el certificado del servidor. El juego aceptaba
# un certificado que hacía de las dos cosas, pero Chromium y el WebView (Note,
# Help) lo rechazan en silencio. Solo la CA se instala en MuMu: el certificado de
# servidor se puede renovar sin volver a tocar el emulador.
mkdir -p "$CERT_DIR"
CA_KEY="$CERT_DIR/konofan-ca.key"
CA_CRT="$CERT_DIR/konofan-ca.crt"
SRV_KEY="$CERT_DIR/konofan.key"
SRV_CRT="$CERT_DIR/konofan.crt"
NEW_CA=0
if [ ! -f "$CA_CRT" ]; then
  openssl req -x509 -newkey rsa:2048 -sha256 -days 3650 -nodes \
    -keyout "$CA_KEY" -out "$CA_CRT" \
    -subj "/CN=Konofan local CA" \
    -addext "basicConstraints=critical,CA:TRUE" \
    -addext "keyUsage=critical,keyCertSign,cRLSign"
  NEW_CA=1
  echo "Creada la CA propia"
fi
# Se rehace el certificado de servidor si la CA es nueva (sustituye al antiguo
# autofirmado), si no existe o si caduca en menos de 30 días. Validez de 397
# días: Chromium rechaza certificados de servidor de más de 398.
if [ "$NEW_CA" = 1 ] || [ ! -f "$SRV_CRT" ] || ! openssl x509 -in "$SRV_CRT" -checkend 2592000 -noout >/dev/null; then
  EXT_FILE="$(mktemp)"
  cat >"$EXT_FILE" <<EXT
basicConstraints=critical,CA:FALSE
keyUsage=critical,digitalSignature,keyEncipherment
extendedKeyUsage=serverAuth
subjectAltName=DNS:$DOMAIN
EXT
  openssl req -new -newkey rsa:2048 -sha256 -nodes \
    -keyout "$SRV_KEY" -out "$CERT_DIR/konofan.csr" -subj "/CN=$DOMAIN"
  openssl x509 -req -in "$CERT_DIR/konofan.csr" -CA "$CA_CRT" -CAkey "$CA_KEY" -CAcreateserial \
    -days 397 -sha256 -extfile "$EXT_FILE" -out "$SRV_CRT"
  rm -f "$EXT_FILE" "$CERT_DIR/konofan.csr"
  echo "Creado el certificado de servidor (caduca: $(openssl x509 -in "$SRV_CRT" -enddate -noout | cut -d= -f2))"
  # Si nginx ya está en marcha, que use el certificado nuevo.
  if systemctl is-active --quiet nginx; then sudo systemctl reload nginx; fi
fi
openssl verify -CAfile "$CA_CRT" "$SRV_CRT"
# En MuMu se instala la CA, con el nombre que Android espera: el hash "antiguo"
# del sujeto.
HASH="$(openssl x509 -in "$CA_CRT" -subject_hash_old -noout)"
cp "$CA_CRT" "$CERT_DIR/$HASH.0"
WIN_OUT=""
if command -v cmd.exe >/dev/null 2>&1; then
  WIN_PROFILE="$(cmd.exe /c 'echo %USERPROFILE%' 2>/dev/null | tr -d '\r')"
  WIN_OUT="$(wslpath "$WIN_PROFILE")/konofan"
  mkdir -p "$WIN_OUT"
  cp "$CERT_DIR/$HASH.0" "$CA_CRT" "$WIN_OUT/"
  echo "Certificado copiado a $WIN_PROFILE\\konofan\\$HASH.0"
fi

step "7/9 nginx (HTTPS en el puerto 443 -> Axel)"
sudo tee /etc/nginx/sites-available/axel >/dev/null <<NGINX
server {
  listen 443 ssl;
  server_name $DOMAIN;

  ssl_certificate     $CERT_DIR/konofan.crt;
  ssl_certificate_key $CERT_DIR/konofan.key;
  client_max_body_size 50m;

  proxy_set_header Host \$host;
  proxy_set_header X-Real-IP \$remote_addr;
  proxy_set_header X-Forwarded-For \$proxy_add_x_forwarded_for;
  proxy_set_header X-Forwarded-Proto \$scheme;

  location /static/ { proxy_pass http://127.0.0.1:2021/; }
  location /api/    { proxy_pass http://127.0.0.1:2020/; }
  # The game launcher can forget the final "/" of https://$DOMAIN/static/; the game then asks for
  # /staticversions/..., /staticbundles/... Send those to /static/...
  location /static  { rewrite ^/static(.*)\$ /static/\$1 last; }
}
NGINX
sudo ln -sf /etc/nginx/sites-available/axel /etc/nginx/sites-enabled/axel
sudo rm -f /etc/nginx/sites-enabled/default
# nginx corre como www-data y necesita leer la clave
chmod o+x "$HOME" "$REPO_DIR" "$CERT_DIR"
chmod o+r "$CERT_DIR/konofan.crt" "$CERT_DIR/konofan.key"
sudo nginx -t
sudo systemctl enable nginx
sudo systemctl restart nginx

step "8/9 Compilando Axel (la primera vez tarda varios minutos)"
(cd "$REPO_DIR" && cargo build --release)

step "9/9 Probabilidades reales del gacha"
RATES_DIR="$RESOURCES_ROOT/gacha_infos/gacha_rates"
mkdir -p "$RATES_DIR"
if [ -z "$(ls -A "$RATES_DIR" 2>/dev/null)" ]; then
  rsync -a --info=progress2 "$RSYNC_BASE/gacha_infos/gacha_rates/" "$RATES_DIR/" \
    || rsync -a --info=progress2 "$RSYNC_BASE/gacha-infos/gacha_rates/" "$RATES_DIR/" \
    || echo "AVISO: no he podido descargar gacha_rates por rsync; las tiradas serán aleatorias hasta importarlos."
fi
if [ -n "$(ls -A "$RATES_DIR" 2>/dev/null)" ]; then
  python3 "$REPO_DIR/migrations/import_gacha_rates.py" "$RATES_DIR" "host=/var/run/postgresql dbname=axel user=$DB_USER" | tail -n 2
fi

cat <<DONE

Listo. Para arrancar el servidor:
  cd $REPO_DIR && ./target/release/axel

Comprobación desde WSL (debe mostrar una clave pública):
  curl --noproxy '*' --cacert $CA_CRT --resolve $DOMAIN:443:127.0.0.1 https://$DOMAIN/static/public.pem

Fichero para MuMu: ${WIN_OUT:+$WIN_OUT/}$HASH.0
DONE
