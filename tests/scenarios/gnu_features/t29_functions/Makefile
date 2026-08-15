SRCS = src/main.c src/util.c inc/util.h
OBJS = $(patsubst %.c,%.o,$(filter %.c,$(SRCS)))
DIRS = $(sort $(dir $(SRCS)))

all:
	@echo "objs=[$(OBJS)]"
	@echo "dirs=[$(DIRS)]"
	@echo "hdrs=[$(notdir $(filter %.h,$(SRCS)))]"
	@echo "count=[$(words $(SRCS))] first=[$(firstword $(SRCS))]"
	@echo "flags=[$(addprefix -I,$(DIRS))]"
