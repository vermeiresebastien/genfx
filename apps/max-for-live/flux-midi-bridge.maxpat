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
				"id" : "obj-midiin",
				"maxclass" : "newobj",
				"text" : "midiin",
				"numinlets" : 0,
				"numoutlets" : 1,
				"outlettype" : [ "int" ],
				"patching_rect" : [ 40.0, 40.0, 50.0, 22.0 ]
			}
		}, {
			"box" : {
				"id" : "obj-midiout",
				"maxclass" : "newobj",
				"text" : "midiout",
				"numinlets" : 1,
				"numoutlets" : 0,
				"outlettype" : [ ],
				"patching_rect" : [ 640.0, 40.0, 56.0, 22.0 ]
			}
		}, {
			"box" : {
				"id" : "obj-midiinfo",
				"maxclass" : "newobj",
				"text" : "midiparse",
				"numinlets" : 1,
				"numoutlets" : 7,
				"outlettype" : [ "int", "int", "int", "int", "int", "list", "list" ],
				"patching_rect" : [ 40.0, 100.0, 80.0, 22.0 ]
			}
		}, {
			"box" : {
				"id" : "obj-nodescript",
				"maxclass" : "newobj",
				"text" : "node.script bridge-osc.js @autostart 1",
				"numinlets" : 1,
				"numoutlets" : 1,
				"outlettype" : [ "" ],
				"patching_rect" : [ 40.0, 260.0, 260.0, 22.0 ]
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
				"id" : "obj-loadbang",
				"maxclass" : "newobj",
				"text" : "loadbang",
				"numinlets" : 1,
				"numoutlets" : 1,
				"outlettype" : [ "bang" ],
				"patching_rect" : [ 320.0, 260.0, 60.0, 22.0 ]
			}
		}, {
			"box" : {
				"id" : "obj-comment",
				"maxclass" : "comment",
				"text" : "Flux MIDI Bridge — midiin passes unchanged to midiout; midiparse (outlet 6 = raw list) feeds bridge-osc.js which encodes + sends OSC to the Flux app. MPE per-note channels preserved.",
				"numinlets" : 1,
				"numoutlets" : 0,
				"outlettype" : [ ],
				"patching_rect" : [ 40.0, 440.0, 780.0, 40.0 ]
			}
		} ],
		"lines" : [ {
			"patchline" : {
				"source" : [ "obj-midiin", 0 ],
				"destination" : [ "obj-midiout", 0 ]
			}
		}, {
			"patchline" : {
				"source" : [ "obj-midiin", 0 ],
				"destination" : [ "obj-midiinfo", 0 ]
			}
		}, {
			"patchline" : {
				"source" : [ "obj-midiinfo", 6 ],
				"destination" : [ "obj-nodescript", 0 ]
			}
		}, {
			"patchline" : {
				"source" : [ "obj-nodescript", 0 ],
				"destination" : [ "obj-udpsend", 0 ]
			}
		}, {
			"patchline" : {
				"source" : [ "obj-loadbang", 0 ],
				"destination" : [ "obj-nodescript", 0 ]
			}
		} ]
	}
}
