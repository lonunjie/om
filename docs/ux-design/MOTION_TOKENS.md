# Motion token seed

Initial semantic values for prototype testing, not immutable API constants.

| Token | Seed duration | Typical use |
|---|---:|---|
| `motion.instant` | 80ms | press/selection acknowledgement |
| `motion.quick` | 160ms | menu/small sheet transition |
| `motion.standard` | 220ms | pane/navigation transition |
| `motion.slow` | 340ms | rare large spatial transition |

Every token has a reduced-motion mapping. Gesture-driven motion does not use a fixed duration while the pointer/finger is actively controlling it.
