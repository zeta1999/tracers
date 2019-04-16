/** This file automatically generated by {{package_name}} {{package_version}}.  Do not edit
 * this file.
 *
 * This file contains native wrappers for the probes defined in trait {{trait_name}}.
 *
 * The source code for that trait is:
 *
 * ```rust
 * {{trait_tokenstream}}
 * ```
 */
#include <sys/sdt.h>

extern "C" {
{% for probe in probes %}
{% include "probe_wrapper.c" %}
{% endfor %}
}
