# Runtime Gaps And Repair Backlog

Дата фиксации: `2026-05-26`

## Closed 1: Human gate continuation stabilized

Факт:

- после repair-pass continuation проверен end-to-end:
  - `waiting_human`
  - `POST /contracts/runs/{run_id}/decision`
  - финальный `completed` + evidence retrieval.

Статус:

- `verified` (после исправления continuity хранения состояния).

Требуется:

- мониторинг повторяемости в следующих smoke-pass.

## Closed 2: `/actions/run` semantics clarified

Факт:

- `POST /actions/run` успешно работает на поддерживаемом connector-path:
  - `connector.http.generic.v1`
  - allowlisted `endpoint_ref`.
- Для неподдерживаемого connector-path возвращается:
  - `403 connector_execution_failed`
  - `connector_denied`.

Статус:

- `verified` как expected protected behavior.

Требуется:

- поддерживать операторские payloads в рамках allowlist и не считать `connector_denied` runtime bug без контекста.

## Gap 3: Schedule operator interface missing

Факт:

- нет подтверждённого маршрута управления расписаниями.

Статус:

- `not implemented`

Требуется:

- отдельный runtime/API design;
- или не обещать это оператору в текущем контуре.

## Gap 4: Runtime bindings visibility partially reduced

Факт:

- добавлен read-only Telegram projection `🔗 Привязки` / `/bindings`;
- live smoke подтверждает `menu:bindings -> 200` для allowed chat;
- projection показывает безопасную сводку без токенов.

Статус:

- `partially verified`

Требуется:

- при необходимости расширить до полной таблицы привязок;
- пока сохранять ограничение как known limitation (видимость неполная).

## Closed 5: Telegram delivery e2e verified

Факт:

- добавлен обязательный smoke:
  - `npm run smoke:telegram-delivery-e2e`
- smoke выполнен с валидными credentials;
- результат: `PASS`;
- Telegram API: `http_status=200`, `ok=true`;
- чат назначения подтверждён по masked `chat_id`.

Статус:

- `verified`.

Требуется:

- держать e2e smoke обязательным перед релизами Telegram-изменений.

## Gap 6: Telegram test menu full action-cycle incomplete

Факт:

- после deploy и webhook-update live smoke подтверждает базовый путь:
  - `/start` -> `200 (menu_sent)`
  - `menu:health` -> `200`
  - `menu:contracts` -> `200`
  - unauthorized chat -> `401`
- полный проход кнопок `run/last/evidence/approve/reject` пока подтверждён только как manual checklist.

Статус:

- `partially verified`.

Требуется:

- выполнить ручной чеклист `tools/smoke_telegram_menu_manual.md` для полного цикла human-gate кнопок;
- зафиксировать PASS/BLOCKED по `run/last/evidence/approve/reject`.
