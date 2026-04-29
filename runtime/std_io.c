#include <stdio.h>
#include <stdlib.h>
#include <stdarg.h>

char *as_format(const char *format, ...) {
    va_list args;
    va_start(args, format);
    va_list args_copy;
    va_copy(args_copy, args);
    int needed = vsnprintf(NULL, 0, format, args);
    va_end(args);

    if (needed < 0) {
        va_end(args_copy);
        return NULL;
    }

    char *buffer = malloc((size_t)needed + 1);
    if (buffer == NULL) {
        va_end(args_copy);
        return NULL;
    }

    vsnprintf(buffer, (size_t)needed + 1, format, args_copy);
    va_end(args_copy);
    return buffer;
}

int as_std_io_output(const char *text) {
    return puts(text);
}
