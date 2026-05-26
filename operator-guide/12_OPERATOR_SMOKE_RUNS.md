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

- `blocked` в текущем прогоне (`2026-05-26`):
  - `telegram_credentials_missing` в окружении запуска smoke.

Что должен увидеть оператор:

- явный `PASS` или `BLOCKED` с причиной;
- без вывода секретов (только `present true/false` и masked values).

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
