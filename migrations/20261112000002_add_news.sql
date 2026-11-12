-- Announcements shown in "Note" (news WebView). Managed from the database instead of being hard-coded.

drop table if exists news cascade;
create table news
(
  id        bigserial primary key,
  title     text        not null,
  -- NewsCategory: 5 = announcement, 2 = event, 3 = recruit, 4 = campaign, 6 = update, 7 = maintenance
  category  integer     not null default 5,
  -- Optional image URL shown above the title
  banner    text        null,
  -- HTML shown when the announcement is opened
  body      text        not null default '',
  priority  integer     not null default 1,
  start_at  timestamptz not null default now(),
  end_at    timestamptz null,
  created_at timestamptz not null default now()
);

insert into news (title, category, body, priority)
values ('¡Bienvenido a tu servidor de Konofan!', 5,
        '<p>Este servidor corre en tu propio PC. Los anuncios de esta sección salen de la tabla <code>news</code> de la base de datos.</p>'
        || '<p>Para añadir uno: <code>insert into news (title, category, body) values (''Título'', 5, ''&lt;p&gt;Texto&lt;/p&gt;'');</code></p>',
        10),
       ('Novedades del servidor', 6,
        '<ul><li>Carrusel de la portada con banners activos.</li><li>Buzón de regalos real.</li><li>Anuncios propios en Note.</li></ul>',
        5);
