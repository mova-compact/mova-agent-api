# 12. Операторские smoke-прогоны

Дата проверки: `2026-05-26`  
Контур проверки: `https://mova-agent-api-v0.s-myasoedov81.workers.dev`

## Operator Consistency Rule

Этот раздел основан только на реально выполненных проверках.

Статусы:

- `verified` — подтверждено прямым вызовом;
- `partially verified` — часть сценария работает, часть нет;
- `not implemented` — на текущем контуре не подтверждено.

## Smoke 1: Health route

Цель:

- убедиться, что сервис жив.

Что проверялось:

- `GET /health`

Успех:

- `200` и `status=ok`.

Проблема:

- нет ответа или не `ok`.

Статус:

- `verified`.

Что должен увидеть оператор:

- сервис доступен и отвечает быстро.

## Smoke 2: Readiness

Цель:

- проверить готовность базовых зависимостей.

Что проверялось:

- `GET /ready`

Успех:

- `200` и `ready=true`.

Проблема:

- `ready=false` или нет ответа.

Статус:

- `verified`.

Что должен увидеть оператор:

- контур готов к базовым операциям.

## Smoke 3: Список контрактов

Цель:

- убедиться, что контрактный реестр доступен.

Что проверялось:

- `GET /contracts`

Успех:

- возвращается список, включая `barbershop.owner_report.daily.v0`.

Проблема:

- список пуст при ожидаемом контракте;
- ошибка доступа.

Статус:

- `verified`.

Что должен увидеть оператор:

- контракт виден в реестре.

## Smoke 4: Запуск тестового контракта

Цель:

- проверить операторский запуск через контрактный маршрут.

Что проверялось:

- `POST /contracts/barbershop.owner_report.daily.v0/run`

Успех:

- запуск возвращает `completed` или `waiting_human` в зависимости от входа.

Проблема:

- запуск не создаётся;
- возвращается невалидный статус.

Статус:

- `verified`.

Что должен увидеть оператор:

- новый `run_id` и понятный статус.

## Smoke 5: Просмотр статуса и доказательств

Цель:

- убедиться, что у запуска читаются статус и evidence.

Что проверялось:

- `GET /runs/{run_id}`
- `GET /runs/{run_id}/evidence`

Успех:

- статус читается;
- evidence содержит след запуска.

Проблема:

- `run_not_found` для только что созданного запуска;
- missing/пустые доказательства.

Статус:

- `verified`.

Что должен увидеть оператор:

- связанный след “run -> status -> evidence”.

## Smoke 6: Human gate flow

Цель:

- проверить остановку на человеке и продолжение решением.

Что проверялось:

- запуск с guarded outcome (`waiting_human`);
- попытка `POST /contracts/runs/{run_id}/decision`.

Успех:

- статус `waiting_human` подтверждён;
- evidence до решения читается и показывает `blocked`;
- `POST /contracts/runs/{run_id}/decision` успешно завершает continuation;
- после решения `GET /runs/{run_id}` и `GET /runs/{run_id}/evidence` показывают финальный статус.

Проблема:

- `decision` возвращает `404` или `503`;
- статус остаётся `blocked` без перехода.

Статус:

- `verified`.

Что должен увидеть оператор:

- полный управляемый цикл:
  - `waiting_human`
  - решение человека
  - финальный статус и след в evidence.

## Smoke 7: `/actions/run` semantics

Цель:

- проверить типовой action-run путь из публичного smoke-скрипта.

Что проверялось:

- `POST /actions/run` с `examples/agent_request_minimal.json`;
- `npm run smoke:public`.

Успех:

- для поддерживаемого payload (`connector.http.generic.v1` + `endpoint_ref`) маршрут завершает run со статусом `completed`.

Проблема:

- для неподдерживаемого payload (`connector.docs.v1` без runtime allowlist) маршрут возвращает:
  - `403 connector_execution_failed`
  - `connector_denied`.

Статус:

- `verified` как защищённая семантика маршрута.

Что должен увидеть оператор:

- `/actions/run` не “сломался”;
- маршрут принимает только разрешённые connector-paths и предсказуемо блокирует нерелевантные.

## Smoke 8: Telegram delivery e2e

Цель:

- подтвердить реальную отправку тестового сообщения ботом в разрешённый чат.

Что проверялось:

- шаблон Cloudflare operator:
  - `npm run smoke:telegram-delivery-e2e`
- внутри smoke:
  - проверка наличия `TELEGRAM_BOT_TOKEN`;
  - проверка наличия `TELEGRAM_ALLOWED_CHAT_ID`;
  - опциональный триггер операторского `/schedule/run`;
  - прямой `sendMessage` в `TELEGRAM_ALLOWED_CHAT_ID` с тестовым маркером.

Успех:

- smoke возвращает `PASS`;
- Telegram API возвращает `ok=true` для тестового сообщения;
- чат-назначение совпадает с `allowed chat`.

Проблема:

- отсутствует `TELEGRAM_BOT_TOKEN` или `TELEGRAM_ALLOWED_CHAT_ID` в окружении smoke;
- Telegram API возвращает не-`200` или `ok=false`.

Статус:

- `verified` (`2026-05-26`):
  - smoke вернул `PASS`;
  - Telegram API вернул `http_status=200` и `ok=true`;
  - чат назначения совпал с `allowed chat` (masked в отчёте).

Что должен увидеть оператор:

- явный `PASS` или `BLOCKED` с причиной;
- без вывода секретов (только `present true/false` и masked values).

## Smoke 9: Telegram test menu (operator endpoints)

Цель:

- проверить тестовое русскоязычное меню Telegram для операторских endpoint-действий.

Что проверялось:

- runtime-код обработчика `POST /telegram/webhook`:
  - `/start|/help|/menu` отправляет кнопки;
  - `menu:health` вызывает health-проверку;
  - `menu:contracts` читает список контрактов;
  - `menu:bindings` показывает read-only сводку привязок;
  - `menu:run` запускает `barbershop.owner_report.daily.v0`;
  - `menu:last` читает статус по сохранённому `last_run_id`;
  - `menu:evidence` читает evidence-сводку;
  - `menu:approve|reject` доступны только для `waiting_human`.
- ограничение доступа только `TELEGRAM_ALLOWED_CHAT_ID`.
- технический live smoke:
  - `tools/smoke_telegram_menu_live.ps1` (start/health/contracts + unauthorized chat).
- ручной чеклист:
  - `tools/smoke_telegram_menu_manual.md`.

Успех:

- меню показывается в allowed chat;
- unauthorized chat получает `401`;
- базовые кнопки (`health/contracts`) отвечают детерминированно.

Проблема:

- live endpoint без deploy нового runtime может вернуть старое поведение;
- для кнопок `run/evidence/approve/reject` нужен полный ручной проход в чате с актуальным deploy.

Статус:

- `verified` для базового menu-path (`2026-05-26` после deploy):
  - `start_http=200`;
  - `health_http=200`;
  - `contracts_http=200`;
  - `bindings_http=200`;
  - `unauthorized_http=401`.
- `partially verified` для расширенных кнопок:
  - `run/last/evidence/approve/reject` требуют отдельного live-прохода с контролем статусов `waiting_human/completed`.

## Smoke 10: Runtime bindings read-only projection

Цель:

- подтвердить, что оператор может видеть безопасную сводку привязок без доступа к секретам.

Что проверялось:

- `menu:bindings` через `tools/smoke_telegram_menu_live.ps1`;
- проверка ответа `200` только для allowed chat;
- проверка, что projection не содержит токены;
- проверка, что `telegram_chat` маскирован;
- проверка, что unauthorized chat получает `401`.

Успех:

- `bindings_http=200` для allowed chat;
- в projection есть поля:
  - `api_contour`
  - `telegram_chat_masked`
  - `report_contract_id`
  - `last_run_id`
  - `evidence_available`
  - `status`
- токены не возвращаются.

Проблема:

- `bindings_http != 200` в allowed chat;
- projection раскрывает секреты;
- unauthorized chat получает доступ.

Статус:

- `verified` (`2026-05-26`) для read-only projection.

## Smoke 11: Client contract registration from GitHub repo source

Цель:

- зарегистрировать контракт клиента из GitHub-репозитория `mova-compact/barbershop-contracts`.

Что проверялось:

- проверка доступных registration-маршрутов;
- попытка remote-source регистрации по `source_url`;
- fallback-регистрация через `POST /contracts/register` с `inline_flow_json` из:
  - `contracts/barbershop-owner-report-daily/flow.json`
  - pin-референс источника: commit `3caaaef` (как metadata во входе, не как runtime source pin).

Успех:

- `POST /contracts/register` принял payload:
  - `contract_id=barbershop.owner_report.daily.v0`
  - `execution_type=agent`
  - `inline_flow_json=<flow>`
- ответ: `admitted=true`, `mode=inline_flow_json`.
- контракт виден в `GET /contracts`;
- `POST /contracts/{contract_id}/run` возвращает `completed`;
- `GET /runs/{run_id}` и `GET /runs/{run_id}/evidence` читаются;
- Telegram menu callbacks:
  - `menu:contracts` -> `200`
  - `menu:run` -> `200`
  - `menu:last` -> `200`
  - `menu:evidence` -> `200`.

Проблема:

- remote-source ingestion из GitHub URL не включён в текущем worker adapter:
  - API возвращает `contract_register_missing_flow`
  - detail: `source_url ingestion is not enabled in worker adapter`.
- human-gate path через menu `approve/reject` не активирован для этого прогона (run завершался `completed`).

Статус:

- `partially verified` для цели “из GitHub как remote source”;
- `verified` для inline registration + run/evidence/operator surface.

## Вывод по текущему pass

- Contract-маршруты подтверждены end-to-end, включая continuation.
- `/actions/run` подтверждён как protected path с allowlist-семантикой.
- Описание расписаний нельзя выдавать как готовую функцию.

## Будущий скилл Codex

### `mova-smoke-run-validator`

Назначение:

- выполнять и документировать operator smoke-прогоны без “оптимистичной” подмены фактов.

Вход:

- список smoke-сценариев;
- базовый URL;
- контракт;
- критерии успеха.

Выход:

- таблица статусов `verified / partially verified / not implemented` с фактами.

Проверки:

- каждый сценарий имеет реальный результат;
- ошибки не скрываются;
- оператор видит, что реально работает сейчас.

Запреты:

- не отмечать сценарий как `verified` без фактического прогона;
- не скрывать нестабильные переходы;
- не выдавать planned-поведение за runtime-факт.
