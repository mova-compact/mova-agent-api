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
- [`13_CONTRACT_REGISTRATION_PLAYBOOK.md`](./13_CONTRACT_REGISTRATION_PLAYBOOK.md) — какой режим регистрации контракта использовать и как проверить результат.
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
- `POST /contracts/runs/{run_id}/decision`
- `GET /runs/{run_id}`
- `GET /runs/{run_id}/evidence`
- `POST /actions/run` для поддерживаемого connector-path (`connector.http.generic.v1` + allowlisted `endpoint_ref`)
- Telegram delivery e2e smoke:
  - `npm run smoke:telegram-delivery-e2e` вернул `PASS` (`2026-05-26`);
  - Telegram API `sendMessage` вернул `ok=true` для allowed chat.
- Telegram visible menu и bot commands:
  - `/start` и кнопки меню подтверждены на live worker;
  - команды `/start`, `/menu`, `/help`, `/bindings`, `/run_owner_report`, `/last_run` зарегистрированы.
- Runtime bindings read-only projection:
  - `menu:bindings` подтверждён на live (`200`) с маскированными полями.

### `partially verified`

- `/actions/run` c неподдерживаемым connector-path:
  - `403 connector_execution_failed`
  - `connector_denied`
  - это ожидаемое protected behavior.
- Telegram test menu для operator endpoints:
  - базовый live-path подтверждён (`/start`, `menu:health`, `menu:contracts`, `menu:bindings`, unauthorized deny);
  - полный live e2e кнопок `run/last/evidence/approve/reject` фиксируется отдельным ручным smoke.

### `blocked`

- нет критичных blocked пунктов в текущем pass;
- если отсутствуют Telegram credentials, e2e smoke автоматически вернёт `BLOCKED`.

### `planned`

- операторское управление расписаниями;
- расширенный операторский экран привязок выполнения;
- расширенный operator UX для контрольной валидации привязок перед запуском.

### `not implemented`

- подтверждённый runtime-интерфейс включения/выключения расписаний в текущем операторском контуре.
