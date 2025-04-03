#ifndef AREXIBO_LIB_H
#define AREXIBO_LIB_H

#include <stdint.h>

extern "C" {

typedef void (* callback)(void *cb_ptr, intptr_t cb_type,
                          intptr_t arg1, intptr_t arg2, intptr_t arg3);

const intptr_t CB_LAYOUT_INIT = 1;
const intptr_t CB_LAYOUT_NEXT = 2;
const intptr_t CB_LAYOUT_PREV = 3;
const intptr_t CB_LAYOUT_JUMP = 4;
const intptr_t CB_SCREENSHOT  = 5;

void setup(const char *base_uri, int inspect, int debug, callback cb, void *cb_ptr);
void run();
void navigate(const char *file);
void screenshot();
void set_title(const char *title);
void set_size(int pos_x, int pos_y, int size_x, int size_y);
void run_js(const char *js);

}

#endif
