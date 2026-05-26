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

## Gap 4: Runtime bindings visibility incomplete

Факт:

- нет подтверждённого операторского экрана полной таблицы привязок выполнения.

Статус:

- `partially verified`

Требуется:

- либо добавить read-only projection привязок;
- либо оставить это как known limitation.

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

## Gap 6: Telegram test menu live verification incomplete

Факт:

- в runtime добавлено тестовое русскоязычное меню operator endpoints;
- добавлен live smoke `tools/smoke_telegram_menu_live.ps1` для `start/health/contracts + unauthorized`;
- live smoke (`2026-05-26`) вернул `PARTIAL`: `start/health/contracts=400`, `unauthorized=400`;
- без deploy новой версии worker live endpoint возвращает старое поведение;
- полный проход кнопок `run/last/evidence/approve/reject` пока подтверждён только как manual checklist.

Статус:

- `partially verified`.

Требуется:

- выполнить deploy c меню;
- прогнать `smoke_telegram_menu_live.ps1`;
- выполнить ручной чеклист `tools/smoke_telegram_menu_manual.md` и зафиксировать итог.
