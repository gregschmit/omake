V = release
ifeq ($(V),release)
  MODE = optimized
else
  MODE = debug
endif
ifdef UNSET
  X = defined
else
  X = undefined
endif
all:
	@echo mode=$(MODE) x=$(X)
