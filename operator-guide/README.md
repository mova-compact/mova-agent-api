# Операторский гайд

Этот каталог содержит русскоязычный операторский гайд для `mova-agent-api` с разделением на подтверждённое поведение и целевые состояния.

## Навигация

- [`00_START_HERE.md`](./00_START_HERE.md) — с чего начать и что это за система.
- [`01_SYSTEM_MAP.md`](./01_SYSTEM_MAP.md) — кто за что отвечает в MOVA.
- [`02_RUN_LIFECYCLE.md`](./02_RUN_LIFECYCLE.md) — как проходит запуск от начала до результата.
- [`03_KEYS_AND_SECRETS.md`](./03_KEYS_AND_SECRETS.md) — ключи, секреты и базовые правила безопасности.
- [`04_POLICIES_AND_PERMISSIONS.md`](./04_POLICIES_AND_PERMISSIONS.md) — что системе разрешено и что запрещено.
- [`05_HUMAN_GATES_AND_APPROVALS.md`](./05_HUMAN_GATES_AND_APPROVALS.md) — когда нужен человек и как принимать решение.
- [`06_TELEGRAM_OPERATOR.md`](./06_TELEGRAM_OPERATOR.md) — Telegram как операторская консоль.
- [`07_AUDIT_AND_EVIDENCE.md`](./07_AUDIT_AND_EVIDENCE.md) — журнал выполнения, отчёт и доказательства.
- [`08_ENVIRONMENTS_AND_RUNTIME_BINDINGS.md`](./08_ENVIRONMENTS_AND_RUNTIME_BINDINGS.md) — как не перепутать контуры и привязки.
- [`09_SCHEDULES_AND_MANUAL_RUNS.md`](./09_SCHEDULES_AND_MANUAL_RUNS.md) — расписания, ручные повторы и риск двойного запуска.
- [`10_ERRORS_AND_RECOVERY.md`](./10_ERRORS_AND_RECOVERY.md) — различие между остановкой, отказом и аварией.
- [`11_HEALTH_AND_SYSTEM_CHECKS.md`](./11_HEALTH_AND_SYSTEM_CHECKS.md) — ежедневные проверки здоровья системы.
- [`12_OPERATOR_SMOKE_RUNS.md`](./12_OPERATOR_SMOKE_RUNS.md) — runtime-verified smoke-проверки оператора.
- [`RUNTIME_GAPS_AND_REPAIR_BACKLOG.md`](./RUNTIME_GAPS_AND_REPAIR_BACKLOG.md) — зафиксированные runtime gaps и обязательные repair-направления.
- [`NEXT_REPAIR_TASKS.md`](./NEXT_REPAIR_TASKS.md) — короткий список следующих repair-задач.
- [`Operator Skills`](../../mova-skills/operator/README.md) — операторские Codex-скиллы для обслуживания контура.

## Что в гайде подтверждено реальными smoke-проверками

### `verified`

- `GET /health`
- `GET /ready`
- `GET /capabilities`
- `GET /contracts`
- `POST /actions/validate`
- `POST /contracts/{contract_id}/run`
- `GET /runs/{run_id}`
- `GET /runs/{run_id}/evidence`

### `partially verified`

- Human gate continuation через `POST /contracts/runs/{run_id}/decision`:
  - остановка `waiting_human` подтверждена;
  - продолжение решения в live-контуре нестабильно (в этом pass получен `404`).
- `POST /actions/run`:
  - маршрут доступен;
  - текущий запуск упирается в `connector_denied` в проверенном контуре.
- Telegram delivery:
  - след Telegram-шага виден в evidence;
  - полноценная операторская проверка доставки в конкретный чат в этом pass не зафиксирована как стабильная.

### `planned`

- операторское управление расписаниями;
- прозрачный операторский экран привязок выполнения;
- стабильный human gate continuation без потери контекста между запросами.

### `not implemented`

- подтверждённый runtime-интерфейс включения/выключения расписаний в текущем операторском контуре.
