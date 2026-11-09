{
	"patcher" : {
		"fileversion" : 1,
		"appversion" : {
			"major" : 9,
			"minor" : 0,
			"revision" : 0,
			"architecture" : "x64"
		},
		"rect" : [ 80.0, 80.0, 860.0, 520.0 ],
		"bglocked" : 0,
		"openinpresentation" : 0,
		"default_font_size" : 12.0,
		"default_font" : "Arial",
		"gridsize" : [ 15.0, 15.0 ],
		"boxes" : [ {
			"box" : {
				"id" : "obj-plugin~",
				"maxclass" : "newobj",
				"text" : "plugin~",
				"numinlets" : 0,
				"numoutlets" : 2,
				"outlettype" : [ "signal", "signal" ],
				"patching_rect" : [ 40.0, 40.0, 60.0, 22.0 ]
			}
		}, {
			"box" : {
				"id" : "obj-plugout~",
				"maxclass" : "newobj",
				"text" : "plugout~",
				"numinlets" : 2,
				"numoutlets" : 2,
				"outlettype" : [ "signal", "signal" ],
				"patching_rect" : [ 640.0, 40.0, 64.0, 22.0 ]
			}
		}, {
			"box" : {
				"id" : "obj-snapshot~",
				"maxclass" : "newobj",
				"text" : "snapshot~ 16",
				"numinlets" : 1,
				"numoutlets" : 1,
				"outlettype" : [ "float" ],
				"patching_rect" : [ 40.0, 120.0, 90.0, 22.0 ]
			}
		}, {
			"box" : {
				"id" : "obj-udpsend",
				"maxclass" : "newobj",
				"text" : "udpsend 127.0.0.1 9000",
				"numinlets" : 1,
				"numoutlets" : 0,
				"outlettype" : [ ],
				"patching_rect" : [ 40.0, 400.0, 160.0, 22.0 ]
			}
		}, {
			"box" : {
				"id" : "obj-comment",
				"maxclass" : "comment",
				"text" : "Flux Audio Bridge — plugin~ passthrough, features over OSC. The bridge OSC encoder lives in node.script bridge-osc.js; udpsend carries the encoded bytes.",
				"numinlets" : 1,
				"numoutlets" : 0,
				"outlettype" : [ ],
				"patching_rect" : [ 40.0, 440.0, 780.0, 40.0 ]
			}
		} ],
		"lines" : [ {
			"patchline" : {
				"source" : [ "obj-plugin~", 0 ],
				"destination" : [ "obj-plugout~", 0 ]
			}
		}, {
			"patchline" : {
				"source" : [ "obj-plugin~", 1 ],
				"destination" : [ "obj-plugout~", 1 ]
			}
		}, {
			"patchline" : {
				"source" : [ "obj-snapshot~", 0 ],
				"destination" : [ "obj-udpsend", 0 ]
			}
		} ]
	}
}
