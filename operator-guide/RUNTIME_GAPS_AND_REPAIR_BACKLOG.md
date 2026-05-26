# Runtime Gaps And Repair Backlog

Дата фиксации: `2026-05-26`

## Gap 1: Human gate continuation unstable

Факт:

- `POST /contracts/runs/{run_id}/decision` в live-проверке возвращал:
  - `404 contract_run_not_found`
  - `404 contract_not_found`

Статус:

- `partially verified`

Требуется:

- отдельная repair-задача.

## Gap 2: `/actions/run` restricted by connector

Факт:

- в live-проверке получен:
  - `403 connector_execution_failed`
  - `connector_denied`

Статус:

- `partially verified`

Требуется:

- либо зафиксировать как ожидаемый policy/connector-результат для этого контура;
- либо исправить connector setup, если это должен быть рабочий путь.

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

