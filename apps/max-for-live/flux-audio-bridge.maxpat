{
	"patcher" : 	{
		"fileversion" : 1,
		"appversion" : 		{
			"major" : 9,
			"minor" : 0,
			"revision" : 9,
			"architecture" : "x64",
			"modernui" : 1
		}
,
		"classnamespace" : "box",
		"rect" : [ 134.0, 134.0, 860.0, 560.0 ],
		"gridsize" : [ 15.0, 15.0 ],
		"boxes" : [ 			{
				"box" : 				{
					"id" : "obj-plugin~",
					"maxclass" : "newobj",
					"numinlets" : 2,
					"numoutlets" : 2,
					"outlettype" : [ "signal", "signal" ],
					"patching_rect" : [ 40.0, 40.0, 60.0, 22.0 ],
					"text" : "plugin~"
				}

			}
, 			{
				"box" : 				{
					"id" : "obj-plugout~",
					"maxclass" : "newobj",
					"numinlets" : 2,
					"numoutlets" : 2,
					"outlettype" : [ "signal", "signal" ],
					"patching_rect" : [ 640.0, 40.0, 64.0, 22.0 ],
					"text" : "plugout~"
				}

			}
, 			{
				"box" : 				{
					"id" : "obj-snapshot~",
					"maxclass" : "newobj",
					"numinlets" : 2,
					"numoutlets" : 1,
					"outlettype" : [ "float" ],
					"patching_rect" : [ 40.0, 120.0, 90.0, 22.0 ],
					"text" : "snapshot~ 16"
				}

			}
, 			{
				"box" : 				{
					"id" : "obj-live-dial",
					"maxclass" : "live.dial",
					"numinlets" : 1,
					"numoutlets" : 2,
					"outlettype" : [ "", "float" ],
					"parameter_enable" : 1,
					"patching_rect" : [ 40.0, 170.0, 41.0, 48.0 ],
					"saved_attribute_attributes" : 					{
						"valueof" : 						{
							"parameter_longname" : "rms",
							"parameter_modmode" : 0,
							"parameter_shortname" : "rms",
							"parameter_type" : 0,
							"parameter_unitstyle" : 0
						}

					}
,
					"varname" : "rms"
				}

			}
, 			{
				"box" : 				{
					"id" : "obj-nodescript",
					"maxclass" : "newobj",
					"numinlets" : 1,
					"numoutlets" : 2,
					"outlettype" : [ "", "" ],
					"patching_rect" : [ 40.0, 280.0, 260.0, 22.0 ],
					"saved_object_attributes" : 					{
						"autostart" : 1,
						"defer" : 0,
						"node_bin_path" : "",
						"npm_bin_path" : "",
						"watch" : 0
					}
,
					"text" : "node.script bridge-osc.js @autostart 1",
					"textfile" : 					{
						"filename" : "bridge-osc.js",
						"flags" : 0,
						"embed" : 0,
						"autowatch" : 1
					}

				}

			}
, 			{
				"box" : 				{
					"id" : "obj-udpsend",
					"maxclass" : "newobj",
					"numinlets" : 1,
					"numoutlets" : 0,
					"patching_rect" : [ 40.0, 420.0, 160.0, 22.0 ],
					"text" : "udpsend 127.0.0.1 9000"
				}

			}
, 			{
				"box" : 				{
					"id" : "obj-loadbang",
					"maxclass" : "newobj",
					"numinlets" : 1,
					"numoutlets" : 1,
					"outlettype" : [ "bang" ],
					"patching_rect" : [ 320.0, 280.0, 60.0, 22.0 ],
					"text" : "loadbang"
				}

			}
, 			{
				"box" : 				{
					"id" : "obj-comment",
					"maxclass" : "comment",
					"numinlets" : 1,
					"numoutlets" : 0,
					"patching_rect" : [ 40.0, 460.0, 1021.0, 20.0 ],
					"text" : "Flux Audio Bridge — plugin~ unity passthrough (audio unchanged); snapshot~ taps a channel; bridge-osc.js encodes features and udpsend transmits OSC to the Flux app. Attach to any track."
				}

			}
 ],
		"lines" : [ 			{
				"patchline" : 				{
					"destination" : [ "obj-nodescript", 0 ],
					"source" : [ "obj-loadbang", 0 ]
				}

			}
, 			{
				"patchline" : 				{
					"destination" : [ "obj-udpsend", 0 ],
					"source" : [ "obj-nodescript", 0 ]
				}

			}
, 			{
				"patchline" : 				{
					"destination" : [ "obj-plugout~", 1 ],
					"source" : [ "obj-plugin~", 1 ]
				}

			}
, 			{
				"patchline" : 				{
					"destination" : [ "obj-plugout~", 0 ],
					"order" : 0,
					"source" : [ "obj-plugin~", 0 ]
				}

			}
, 			{
				"patchline" : 				{
					"destination" : [ "obj-snapshot~", 0 ],
					"order" : 1,
					"source" : [ "obj-plugin~", 0 ]
				}

			}
, 			{
				"patchline" : 				{
					"destination" : [ "obj-nodescript", 0 ],
					"source" : [ "obj-snapshot~", 0 ]
				}

			}
 ],
		"parameters" : 		{
			"obj-live-dial" : [ "rms", "rms", 0 ],
			"parameterbanks" : 			{
				"0" : 				{
					"index" : 0,
					"name" : "",
					"parameters" : [ "-", "-", "-", "-", "-", "-", "-", "-" ],
					"buttons" : [ "-", "-", "-", "-", "-", "-", "-", "-" ]
				}

			}
,
			"inherited_shortname" : 1
		}
,
		"dependency_cache" : [ 			{
				"name" : "bridge-osc.js",
				"bootpath" : "D:/CURSOR LOCAL PROJECTS/VIDEO - GENFX/apps/max-for-live",
				"patcherrelativepath" : ".",
				"type" : "TEXT",
				"implicit" : 1
			}
 ],
		"autosave" : 0,
		"oscreceiveudpport" : 0
	}

}
