# 13. Playbook регистрации контрактов

Дата: `2026-05-26`

## Цель

Дать оператору короткий и однозначный порядок регистрации контракта в MOVA без смешивания технического smoke и клиентского production-source.

## Режим A: быстрый технический smoke (`inline_flow_json`)

Когда использовать:

- быстрая проверка runtime;
- внутренний инженерный smoke;
- отладка формы payload.

Вход:

- `contract_id`;
- `execution_type`;
- `inline_flow_json`.

Что проверяется:

- API принимает регистрацию;
- контракт виден в `GET /contracts`;
- запуск создаёт `run_id`.

PASS:

- `POST /contracts/register` -> `admitted=true`;
- контракт читается через `/contracts`;
- `run` и `evidence` читаются.

Риски:

- это не клиентский источник истины;
- payload может быть не привязан к зафиксированному GitHub commit.

Почему не основной client-source путь:

- нет обязательной связи с pinned commit в GitHub репозитории клиента.

## Режим B: локальный клиентский repo (`local_packaged`)

Когда использовать:

- проверка контракта из локальной рабочей копии перед push;
- операторская проверка после локальных изменений.

Команда (из `clients/barbershop-contracts`):

- `node scripts/register_contract_local.mjs`

Вход:

- локальная папка контракта;
- `MOVA_API_BASE_URL`;
- `MOVA_API_TOKEN`.

PASS:

- registration `admitted`;
- контракт виден в `/contracts`;
- `POST /contracts/{contract_id}/run` работает;
- `GET /runs/{run_id}/evidence` читается.

Риски:

- локальное состояние может отличаться от GitHub;
- это не воспроизводимый production-source, если commit не запушен.

## Режим C: GitHub repo клиента (`github_source`)

Когда использовать:

- основной путь для client/prod source.

Вход:

- `source_url`;
- `commit_sha`;
- `contract_path`;
- `contract_id`.

Правила:

- `commit_sha` обязателен;
- floating branch (`main`, `master`, `latest`) запрещён.

PASS:

- registration `admitted`;
- source metadata сохранено (`source_type/source_url/commit_sha/contract_path`);
- контракт виден в `/contracts`;
- run работает;
- evidence читается;
- Telegram показывает `contracts/run/last/evidence`.

## Таблица выбора режима

| Сценарий | Использовать |
| --- | --- |
| Быстрая техническая проверка runtime | `inline_flow_json` |
| Проверка локального repo перед push | `local_packaged` |
| Клиентский источник истины в GitHub | `github_source` |

## Источник истины

- Для клиента source of truth: GitHub repo + pinned `commit_sha`.
- Local repo: рабочая копия для подготовки изменений.
- MOVA registry: зарегистрированный admitted snapshot.
- Telegram: операторская видимость, не источник контракта.

## Что нельзя делать

- регистрировать production-контракт с floating `main`;
- регистрировать неизвестный GitHub URL;
- считать local copy production-source, если она не запушена;
- коммитить `.env` и `.dev.vars`;
- отправлять секреты в contract package;
- путать `inline_flow_json` smoke с клиентским production-контуром.

## Минимальная проверка после регистрации

Для любого режима:

1. `GET /contracts`
2. `POST /contracts/{contract_id}/run`
3. `GET /runs/{run_id}`
4. `GET /runs/{run_id}/evidence`
5. Telegram:
   - `contracts`
   - `run`
   - `last`
   - `evidence`
