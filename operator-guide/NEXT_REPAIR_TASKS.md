# Next Repair Tasks

| Проблема | Риск для оператора | Минимальная проверка | Минимальное исправление | Приоритет |
| --- | --- | --- | --- | --- |
| Нет интерфейса расписаний | Оператор не может штатно управлять запуском по времени | Проверить наличие runtime/API маршрутов расписаний | Добавить минимальный read-only/управляющий API или убрать обещание из target-state | P1 |
| Неполная видимость runtime bindings | Риск запуска не в том контуре | Сопоставить среду и evidence на нескольких запусках | Добавить read-only projection привязок выполнения | P1 |
| Telegram test menu live verification incomplete | Оператор может видеть старое поведение меню или неполный набор кнопок | Задеплоить меню-версию и прогнать `tools/smoke_telegram_menu_live.ps1` + manual checklist | Зафиксировать PASS/BLOCKED по кнопкам `run/last/evidence/approve/reject` и обновить guide/skills | P1 |

## Naming mismatch note

| Тема | Текущее состояние | Минимальное действие | Приоритет |
| --- | --- | --- | --- |
| Telegram env naming между template-линиями | Cloudflare template использует `TELEGRAM_ALLOWED_CHAT_ID`, pydantic template опирается на `OPERATOR_CHAT_ID`/`OPERATOR_CONVERSATION_ID` | Зафиксировать mapping в docs, без мгновенного cross-template refactor | P2 |

## Закрыто в REPAIR_PASS_01

| Проблема | Результат | Проверка |
| --- | --- | --- |
| Human gate continuation unstable | Стабилизировано | `waiting_human -> decision -> completed` подтверждено на live |
| `/actions/run` connector ambiguity | Семантика прояснена как protected path | поддерживаемый payload проходит, неподдерживаемый предсказуемо даёт `connector_denied` |
