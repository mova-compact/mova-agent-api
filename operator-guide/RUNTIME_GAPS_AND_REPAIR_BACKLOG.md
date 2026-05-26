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

## Gap 5: Telegram delivery e2e partially verified

Факт:

- в evidence есть Telegram-step;
- но нет обязательного e2e-check доставки в конкретный чат.

Статус:

- `partially verified`

Требуется:

- отдельный smoke-прогон доставки.
