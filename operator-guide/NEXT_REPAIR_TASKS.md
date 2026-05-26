# Next Repair Tasks

| Проблема | Риск для оператора | Минимальная проверка | Минимальное исправление | Приоритет |
| --- | --- | --- | --- | --- |
| Нет интерфейса расписаний | Оператор не может штатно управлять запуском по времени | Проверить наличие runtime/API маршрутов расписаний | Добавить минимальный read-only/управляющий API или убрать обещание из target-state | P1 |
| Неполная расширенная видимость runtime bindings | Есть read-only сводка, но нет полной таблицы всех привязок | Проверить `menu:bindings` и сопоставить с evidence/capabilities на нескольких запусках | Уточнить, нужна ли полная таблица; если да — добавить расширенную read-only projection | P2 |
| Telegram test menu full action-cycle not fully verified | Оператор видит меню, но часть кнопок может вести себя неочевидно в human-gate сценарии | Прогнать `tools/smoke_telegram_menu_manual.md` на `run/last/evidence/approve/reject` | Зафиксировать PASS/BLOCKED по полному циклу `waiting_human -> decision` из меню | P1 |
| Private GitHub source auth-flow missing | Нельзя ingest-контракт из приватного GitHub repo без отдельного auth-механизма | Проверить `POST /contracts/register` c private source и зафиксировать `blocked` | Добавить минимальный token-based fetch flow для private repos | P2 |

## Naming mismatch note

| Тема | Текущее состояние | Минимальное действие | Приоритет |
| --- | --- | --- | --- |
| Telegram env naming между template-линиями | Cloudflare template использует `TELEGRAM_ALLOWED_CHAT_ID`, pydantic template опирается на `OPERATOR_CHAT_ID`/`OPERATOR_CONVERSATION_ID` | Зафиксировать mapping в docs, без мгновенного cross-template refactor | P2 |

## Закрыто в REPAIR_PASS_01

| Проблема | Результат | Проверка |
| --- | --- | --- |
| Human gate continuation unstable | Стабилизировано | `waiting_human -> decision -> completed` подтверждено на live |
| `/actions/run` connector ambiguity | Семантика прояснена как protected path | поддерживаемый payload проходит, неподдерживаемый предсказуемо даёт `connector_denied` |
