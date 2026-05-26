# Next Repair Tasks

| Проблема | Риск для оператора | Минимальная проверка | Минимальное исправление | Приоритет |
| --- | --- | --- | --- | --- |
| Нет интерфейса расписаний | Оператор не может штатно управлять запуском по времени | Проверить наличие runtime/API маршрутов расписаний | Добавить минимальный read-only/управляющий API или убрать обещание из target-state | P1 |
| Неполная видимость runtime bindings | Риск запуска не в том контуре | Сопоставить среду и evidence на нескольких запусках | Добавить read-only projection привязок выполнения | P1 |
| Telegram e2e smoke blocked (credentials missing in smoke env) | Нельзя подтвердить доставку в конкретный allowed chat | `npm run smoke:telegram-delivery-e2e` и проверить `PASS` | Передать smoke-доступ к `TELEGRAM_BOT_TOKEN` + `TELEGRAM_ALLOWED_CHAT_ID`, затем повторить smoke и вручную сверить чат | P1 |

## Naming mismatch note

| Тема | Текущее состояние | Минимальное действие | Приоритет |
| --- | --- | --- | --- |
| Telegram env naming между template-линиями | Cloudflare template использует `TELEGRAM_ALLOWED_CHAT_ID`, pydantic template опирается на `OPERATOR_CHAT_ID`/`OPERATOR_CONVERSATION_ID` | Зафиксировать mapping в docs, без мгновенного cross-template refactor | P2 |

## Закрыто в REPAIR_PASS_01

| Проблема | Результат | Проверка |
| --- | --- | --- |
| Human gate continuation unstable | Стабилизировано | `waiting_human -> decision -> completed` подтверждено на live |
| `/actions/run` connector ambiguity | Семантика прояснена как protected path | поддерживаемый payload проходит, неподдерживаемый предсказуемо даёт `connector_denied` |
