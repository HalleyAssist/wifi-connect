$(function(){
	var networks = undefined, is_manual = false;

	function showHideEnterpriseSettings() {
		var security = $(this).find(':selected').attr('data-security');
		if(security === 'enterprise') {
			$('#identity-group').show();
		} else {
			$('#identity-group').hide();
		}
	}

	function setManual(toValue){
		if(toValue){
			$('#identity-group, #ssid-manual, #btn-ssid-manual').show();
			$('#ssid-select, #btn-ssid-list').hide();
		}else{
			$('#ssid-select, #btn-ssid-list').show();
			$('#ssid-manual, #btn-ssid-manual').hide();
			showHideEnterpriseSettings();
		}
		is_manual = toValue;
	}

	$('#btn-ssid-manual').click(function(ev){
		setManual(true)
		ev.preventDefault();
	})

	$('#btn-ssid-list').click(function(ev){
		setManual(false)
		ev.preventDefault();
	})

	$('#ssid-select').change(showHideEnterpriseSettings);

	$.get("networks", function(data){
		if(data.length === 0){
			$('.before-submit').hide();
			$('#no-networks-message').removeClass('hidden');
		} else {
			networks = JSON.parse(data);
			$.each(networks, function(i, val){
				$('#ssid-select').append(
					$('<option>')
						.text(val.ssid)
						.attr('val', val.ssid)
						.attr('data-security', val.security)
				);
			});

			jQuery.proxy(showHideEnterpriseSettings, $('#ssid-select'))();
		}
	});

	$('#connect-form').submit(function(ev){
		$('input[name="ssid"]').val($(is_manual?"#ssid-manual":"#ssid-select").val());

		$.post('connect', $('#connect-form').serialize(), function(data){
			$('.before-submit').hide();
			$('#submit-message').removeClass('hidden');
		});
		ev.preventDefault();
	});
});
